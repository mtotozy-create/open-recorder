export function formatFileSize(bytes: number): string {
  if (bytes === 0) {
    return "0 B";
  }

  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const sizeIndex = Math.min(
    sizes.length - 1,
    Math.floor(Math.log(bytes) / Math.log(k))
  );

  return `${Number.parseFloat((bytes / k ** sizeIndex).toFixed(2))} ${sizes[sizeIndex]}`;
}
