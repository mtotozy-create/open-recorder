function estimateTextDisplayUnits(input: string): number {
  const normalized = input.replace(/\s+/g, " ").trim();
  if (!normalized) {
    return 0;
  }
  let units = 0;
  for (const char of normalized) {
    if (char === " ") {
      units += 0.5;
      continue;
    }
    const code = char.charCodeAt(0);
    const isWideGlyph =
      (code >= 0x2e80 && code <= 0x9fff) ||
      (code >= 0xac00 && code <= 0xd7af) ||
      (code >= 0x3040 && code <= 0x30ff) ||
      (code >= 0xff01 && code <= 0xff60);
    units += isWideGlyph ? 2 : 1;
  }
  return units;
}

function normalizeColumnPercentages(
  rawPercentages: number[],
  minPercent: number,
  maxPercent: number
): number[] {
  const adjusted = rawPercentages.map((value) =>
    Math.min(maxPercent, Math.max(minPercent, value))
  );
  const tolerance = 0.01;
  for (let round = 0; round < 24; round += 1) {
    const sum = adjusted.reduce((acc, value) => acc + value, 0);
    const diff = 100 - sum;
    if (Math.abs(diff) <= tolerance) {
      break;
    }
    if (diff > 0) {
      const candidates = adjusted
        .map((value, index) => ({ index, capacity: maxPercent - value }))
        .filter((item) => item.capacity > tolerance);
      const capacitySum = candidates.reduce((acc, item) => acc + item.capacity, 0);
      if (capacitySum <= tolerance) {
        break;
      }
      for (const item of candidates) {
        adjusted[item.index] = Math.min(
          maxPercent,
          adjusted[item.index] + (diff * item.capacity) / capacitySum
        );
      }
    } else {
      const need = -diff;
      const candidates = adjusted
        .map((value, index) => ({ index, capacity: value - minPercent }))
        .filter((item) => item.capacity > tolerance);
      const capacitySum = candidates.reduce((acc, item) => acc + item.capacity, 0);
      if (capacitySum <= tolerance) {
        break;
      }
      for (const item of candidates) {
        adjusted[item.index] = Math.max(
          minPercent,
          adjusted[item.index] - (need * item.capacity) / capacitySum
        );
      }
    }
  }
  const finalSum = adjusted.reduce((acc, value) => acc + value, 0);
  if (adjusted.length > 0 && Math.abs(100 - finalSum) > tolerance) {
    adjusted[0] += 100 - finalSum;
  }
  return adjusted;
}

function applyAdaptiveTableColumnWidths(table: HTMLTableElement): void {
  const rows = Array.from(table.querySelectorAll("tr"));
  const columnCount = rows.reduce((max, row) => {
    const cells = Array.from(row.children).filter((child) => {
      const tag = child.tagName.toUpperCase();
      return tag === "TH" || tag === "TD";
    });
    return Math.max(max, cells.length);
  }, 0);
  if (columnCount <= 0) {
    return;
  }

  const columnScores = new Array<number>(columnCount).fill(4);
  rows.forEach((row, rowIndex) => {
    const cells = Array.from(row.children).filter((child) => {
      const tag = child.tagName.toUpperCase();
      return tag === "TH" || tag === "TD";
    }) as HTMLElement[];
    cells.forEach((cell, index) => {
      const text = cell.innerText || cell.textContent || "";
      const units = Math.max(1, Math.min(160, estimateTextDisplayUnits(text)));
      const weighted = rowIndex === 0 ? units * 1.15 : units;
      columnScores[index] = Math.max(columnScores[index], weighted);
    });
  });

  const effectiveScores = columnScores.map((score) => Math.sqrt(score) + 2);
  const totalEffective = effectiveScores.reduce((acc, value) => acc + value, 0);
  const rawPercentages = effectiveScores.map((value) => (value / totalEffective) * 100);

  const suggestedMin =
    columnCount >= 6 ? 7 : columnCount === 5 ? 9 : columnCount === 4 ? 11 : columnCount === 3 ? 15 : 22;
  const minPercent = Math.max(4, Math.min(suggestedMin, Math.floor(100 / columnCount) - 1));
  const maxPercent = columnCount <= 2 ? 78 : columnCount === 3 ? 60 : columnCount === 4 ? 48 : 42;
  const normalizedPercentages = normalizeColumnPercentages(rawPercentages, minPercent, maxPercent);

  table.querySelectorAll("colgroup[data-or-pdf-colgroup='1']").forEach((node) => node.remove());
  const colgroup = document.createElement("colgroup");
  colgroup.setAttribute("data-or-pdf-colgroup", "1");
  for (const percentage of normalizedPercentages) {
    const col = document.createElement("col");
    col.style.width = `${percentage.toFixed(2)}%`;
    colgroup.appendChild(col);
  }
  table.insertBefore(colgroup, table.firstChild);
}

export function sanitizeFileNameSegment(input: string): string {
  return input
    .replace(/[<>:"/\\|?*\u0000-\u001F]/g, " ")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, 80);
}

export type ExportElementToPdfOptions = {
  rootSelector: string;
  fileName: string;
  missingRootError: string;
  footerText?: string;
};

export async function exportElementToPdf(options: ExportElementToPdfOptions): Promise<string> {
  if (typeof window === "undefined" || typeof document === "undefined") {
    throw new Error("pdf export is unavailable in this environment");
  }

  const root = document.querySelector<HTMLElement>(options.rootSelector);
  if (!root) {
    throw new Error(options.missingRootError);
  }

  let captureNode: HTMLElement | undefined;
  let exportRootNode: HTMLDivElement | undefined;

  try {
    const [{ default: html2canvas }, { jsPDF }] = await Promise.all([
      import("html2canvas"),
      import("jspdf")
    ]);

    const pdf = new jsPDF({
      orientation: "portrait",
      unit: "pt",
      format: "a4"
    });
    const pageWidth = pdf.internal.pageSize.getWidth();
    const pageHeight = pdf.internal.pageSize.getHeight();
    const margin = 28;
    const footerHeight = options.footerText ? 22 : 0;
    const contentWidth = pageWidth - margin * 2;
    const contentHeight = pageHeight - margin * 2 - footerHeight;
    const captureWidthPx = Math.max(640, Math.round((contentWidth * 96) / 72));

    exportRootNode = document.createElement("div");
    exportRootNode.style.position = "fixed";
    exportRootNode.style.left = "-100000px";
    exportRootNode.style.top = "0";
    exportRootNode.style.width = `${captureWidthPx}px`;
    exportRootNode.style.maxWidth = `${captureWidthPx}px`;
    exportRootNode.style.height = "auto";
    exportRootNode.style.maxHeight = "none";
    exportRootNode.style.overflow = "visible";
    exportRootNode.style.background = "#ffffff";
    exportRootNode.style.boxSizing = "border-box";
    exportRootNode.style.padding = "0";
    exportRootNode.style.margin = "0";

    captureNode = root.cloneNode(true) as HTMLElement;
    captureNode.style.position = "static";
    captureNode.style.width = "100%";
    captureNode.style.maxWidth = "100%";
    captureNode.style.maxHeight = "none";
    captureNode.style.height = "auto";
    captureNode.style.overflow = "visible";
    captureNode.style.background = "#ffffff";
    captureNode.style.padding = "0";
    captureNode.style.margin = "0";
    captureNode.style.color = "#111827";

    const markdownTables = captureNode.querySelectorAll<HTMLTableElement>("table");
    for (const table of markdownTables) {
      table.style.display = "table";
      table.style.width = "100%";
      table.style.maxWidth = "100%";
      table.style.tableLayout = "fixed";
      table.style.overflow = "visible";
      table.style.overflowX = "visible";
      table.style.overflowY = "visible";
      table.style.whiteSpace = "normal";
      table.style.wordBreak = "break-word";
      applyAdaptiveTableColumnWidths(table);
    }
    const tableParents = captureNode.querySelectorAll<HTMLElement>("table, thead, tbody, tr");
    for (const item of tableParents) {
      item.style.maxWidth = "100%";
      item.style.overflow = "visible";
    }
    const tableCells = captureNode.querySelectorAll<HTMLElement>("th, td");
    for (const cell of tableCells) {
      cell.style.whiteSpace = "normal";
      cell.style.wordBreak = "break-word";
      cell.style.overflowWrap = "anywhere";
      cell.style.maxWidth = "none";
    }
    const textBlocks = captureNode.querySelectorAll<HTMLElement>("p, li, blockquote");
    for (const block of textBlocks) {
      block.style.whiteSpace = block.tagName === "P" ? "pre-line" : "normal";
      block.style.wordBreak = "break-word";
      block.style.overflowWrap = "anywhere";
    }
    const preBlocks = captureNode.querySelectorAll<HTMLElement>("pre");
    for (const pre of preBlocks) {
      pre.style.whiteSpace = "pre-wrap";
      pre.style.wordBreak = "break-word";
      pre.style.overflowWrap = "anywhere";
    }
    exportRootNode.appendChild(captureNode);
    document.body.appendChild(exportRootNode);

    await new Promise<void>((resolve) => {
      window.requestAnimationFrame(() => resolve());
    });

    const canvas = await html2canvas(exportRootNode, {
      backgroundColor: "#ffffff",
      scale: Math.min(window.devicePixelRatio || 2, 2),
      useCORS: true,
      logging: false,
      width: exportRootNode.scrollWidth,
      windowWidth: exportRootNode.scrollWidth
    });

    const pageHeightPx = Math.max(1, Math.floor((contentHeight * canvas.width) / contentWidth));
    let renderedHeightPx = 0;
    let pageIndex = 0;
    while (renderedHeightPx < canvas.height) {
      const sliceHeightPx = Math.min(pageHeightPx, canvas.height - renderedHeightPx);
      const pageCanvas = document.createElement("canvas");
      pageCanvas.width = canvas.width;
      pageCanvas.height = sliceHeightPx;
      const pageCtx = pageCanvas.getContext("2d");
      if (!pageCtx) {
        throw new Error("failed to render PDF page");
      }
      pageCtx.fillStyle = "#ffffff";
      pageCtx.fillRect(0, 0, pageCanvas.width, pageCanvas.height);
      pageCtx.drawImage(
        canvas,
        0,
        renderedHeightPx,
        canvas.width,
        sliceHeightPx,
        0,
        0,
        pageCanvas.width,
        pageCanvas.height
      );

      const pageImageData = pageCanvas.toDataURL("image/png");
      const renderedPageHeight = (sliceHeightPx * contentWidth) / canvas.width;
      if (pageIndex > 0) {
        pdf.addPage();
      }
      pdf.addImage(
        pageImageData,
        "PNG",
        margin,
        margin,
        contentWidth,
        renderedPageHeight,
        undefined,
        "FAST"
      );

      renderedHeightPx += sliceHeightPx;
      pageIndex += 1;
    }

    if (options.footerText && pageIndex > 0) {
      const footerY = pageHeight - 12;
      pdf.setFont("helvetica", "normal");
      pdf.setFontSize(8);
      pdf.setTextColor(107, 114, 128);
      for (let pageNumber = 1; pageNumber <= pageIndex; pageNumber += 1) {
        pdf.setPage(pageNumber);
        pdf.text(options.footerText, margin, footerY);
        pdf.text(`${pageNumber} / ${pageIndex}`, pageWidth - margin, footerY, { align: "right" });
      }
    }

    const finalFileName = options.fileName.toLowerCase().endsWith(".pdf")
      ? options.fileName
      : `${options.fileName}.pdf`;
    pdf.save(finalFileName);
    return finalFileName;
  } finally {
    if (exportRootNode?.parentElement) {
      exportRootNode.parentElement.removeChild(exportRootNode);
    } else if (captureNode?.parentElement) {
      captureNode.parentElement.removeChild(captureNode);
    }
  }
}
