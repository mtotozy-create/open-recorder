use std::{fs, path::Path, time::Duration};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::{
    blocking::Client,
    header::{CONTENT_TYPE, DATE},
    Url,
};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

#[derive(Debug, Clone)]
pub struct AliyunOssConfig {
    pub access_key_id: String,
    pub access_key_secret: String,
    pub endpoint: String,
    pub bucket: String,
    pub path_prefix: Option<String>,
    pub signed_url_ttl_seconds: u64,
}

pub fn upload_segments_and_sign_urls(
    segment_paths: &[String],
    session_id: &str,
    config: &AliyunOssConfig,
) -> Result<Vec<String>, String> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(3600))
        .build()
        .map_err(|error| format!("failed to create http client: {error}"))?;

    let base_url = build_bucket_base_url(&config.endpoint, &config.bucket)?;
    let mut file_urls = Vec::with_capacity(segment_paths.len());

    for (index, segment_path) in segment_paths.iter().enumerate() {
        let local_path = Path::new(segment_path);
        let file_name = local_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| format!("invalid segment path: {segment_path}"))?;

        let object_key = build_object_key(session_id, file_name, config.path_prefix.as_deref());
        upload_object(&client, local_path, &object_key, config, &base_url).map_err(|error| {
            format!(
                "failed to upload segment {index} {} to OSS: {error}",
                local_path.display()
            )
        })?;
        let signed_url = build_signed_get_url(&object_key, config, &base_url).map_err(|error| {
            format!(
                "failed to sign url for segment {index} {}: {error}",
                local_path.display()
            )
        })?;
        file_urls.push(signed_url);
    }

    Ok(file_urls)
}

fn upload_object(
    client: &Client,
    local_path: &Path,
    object_key: &str,
    config: &AliyunOssConfig,
    base_url: &Url,
) -> Result<(), String> {
    let bytes = fs::read(local_path).map_err(|error| {
        format!(
            "failed to read local audio file {}: {error}",
            local_path.display()
        )
    })?;
    let content_type = mime_from_file_name(
        local_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default(),
    );
    let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    let canonical_resource = format!(
        "/{}/{}",
        config.bucket.trim(),
        object_key.trim_start_matches('/')
    );
    let string_to_sign = format!("PUT\n\n{content_type}\n{date}\n{canonical_resource}");
    let signature = sign_oss_string(&config.access_key_secret, &string_to_sign)?;
    let authorization = format!("OSS {}:{signature}", config.access_key_id.trim());
    let object_url = build_object_url(base_url, object_key);

    let response = client
        .put(object_url)
        .header(DATE, date)
        .header(CONTENT_TYPE, content_type)
        .header("Authorization", authorization)
        .body(bytes)
        .send()
        .map_err(|error| format!("request failed: {error}"))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(format!("request failed with {status}: {body}"));
    }

    Ok(())
}

fn build_signed_get_url(
    object_key: &str,
    config: &AliyunOssConfig,
    base_url: &Url,
) -> Result<String, String> {
    let expires = Utc::now().timestamp() + config.signed_url_ttl_seconds.max(60) as i64;
    let canonical_resource = format!(
        "/{}/{}",
        config.bucket.trim(),
        object_key.trim_start_matches('/')
    );
    let string_to_sign = format!("GET\n\n\n{expires}\n{canonical_resource}");
    let signature = sign_oss_string(&config.access_key_secret, &string_to_sign)?;

    let mut url = build_object_url(base_url, object_key);
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("OSSAccessKeyId", config.access_key_id.trim());
        query.append_pair("Expires", &expires.to_string());
        query.append_pair("Signature", &signature);
    }
    Ok(url.to_string())
}

fn build_bucket_base_url(endpoint: &str, bucket: &str) -> Result<Url, String> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Err("bailian OSS endpoint is empty".to_string());
    }
    let bucket = bucket.trim();
    if bucket.is_empty() {
        return Err("bailian OSS bucket is empty".to_string());
    }

    let mut base_url = if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        Url::parse(endpoint).map_err(|error| format!("invalid OSS endpoint URL: {error}"))?
    } else {
        Url::parse(&format!("https://{endpoint}"))
            .map_err(|error| format!("invalid OSS endpoint: {error}"))?
    };

    if base_url.host_str().is_none() {
        return Err("OSS endpoint must contain host".to_string());
    }
    if !base_url.path().trim_matches('/').is_empty() {
        return Err("OSS endpoint should not contain path".to_string());
    }

    let host = base_url.host_str().unwrap_or_default();
    let bucket_host = if host.starts_with(&format!("{bucket}.")) {
        host.to_string()
    } else {
        format!("{bucket}.{host}")
    };
    base_url
        .set_host(Some(&bucket_host))
        .map_err(|_| "invalid OSS bucket or endpoint host".to_string())?;

    base_url.set_path("/");
    base_url.set_query(None);
    base_url.set_fragment(None);
    Ok(base_url)
}

fn build_object_url(base_url: &Url, object_key: &str) -> Url {
    let mut url = base_url.clone();
    let normalized_key = object_key.trim().trim_start_matches('/');
    url.set_path(&format!("/{normalized_key}"));
    url
}

fn build_object_key(session_id: &str, file_name: &str, path_prefix: Option<&str>) -> String {
    let prefix = path_prefix
        .map(normalize_key_part)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "open-recorder".to_string());
    let session = normalize_key_part(session_id);
    let file = normalize_key_part(file_name);

    format!("{prefix}/{session}/segments/{file}")
}

fn normalize_key_part(value: &str) -> String {
    value
        .replace('\\', "/")
        .split('/')
        .map(str::trim)
        .filter(|segment| !segment.is_empty() && *segment != ".")
        .collect::<Vec<_>>()
        .join("/")
}

fn sign_oss_string(secret: &str, content: &str) -> Result<String, String> {
    let mut mac = HmacSha1::new_from_slice(secret.as_bytes())
        .map_err(|error| format!("failed to initialize HMAC: {error}"))?;
    mac.update(content.as_bytes());
    Ok(BASE64_STANDARD.encode(mac.finalize().into_bytes()))
}

fn mime_from_file_name(file_name: &str) -> &'static str {
    if file_name.ends_with(".webm") {
        "audio/webm"
    } else if file_name.ends_with(".wav") {
        "audio/wav"
    } else if file_name.ends_with(".ogg") {
        "audio/ogg"
    } else if file_name.ends_with(".mp3") {
        "audio/mpeg"
    } else if file_name.ends_with(".m4a") {
        "audio/mp4"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::{build_bucket_base_url, build_object_key};

    #[test]
    fn build_object_key_uses_default_prefix() {
        assert_eq!(
            build_object_key("session-1", "segment-2026-01-01.wav", None),
            "open-recorder/session-1/segments/segment-2026-01-01.wav"
        );
    }

    #[test]
    fn build_object_key_normalizes_segments() {
        assert_eq!(
            build_object_key("/abc//", "/segment.wav", Some("custom//prefix/")),
            "custom/prefix/abc/segments/segment.wav"
        );
    }

    #[test]
    fn build_bucket_base_url_prefixes_bucket_host() {
        let url = build_bucket_base_url("https://oss-cn-beijing.aliyuncs.com", "my-bucket")
            .expect("url should be valid");
        assert_eq!(
            url.as_str(),
            "https://my-bucket.oss-cn-beijing.aliyuncs.com/"
        );
    }
}
