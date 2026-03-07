use std::{fs, path::Path, time::Duration};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use chrono::Utc;
use hmac::{Hmac, Mac};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use reqwest::{
    blocking::Client,
    header::{CONTENT_TYPE, DATE},
    Url,
};
use sha1::Sha1;
use sha2::{Digest, Sha256};

type HmacSha1 = Hmac<Sha1>;
type HmacSha256 = Hmac<Sha256>;

const AWS_REGION_AUTO: &str = "auto";
const AWS_SERVICE_S3: &str = "s3";
const AWS_REQUEST_TYPE: &str = "aws4_request";
const AWS_ALGORITHM: &str = "AWS4-HMAC-SHA256";

const AWS_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OssProviderKind {
    Aliyun,
    R2,
}

#[derive(Debug, Clone)]
pub struct OssConfig {
    pub kind: OssProviderKind,
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
    config: &OssConfig,
) -> Result<Vec<String>, String> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(3600))
        .build()
        .map_err(|error| format!("failed to create http client: {error}"))?;

    let base_url = build_base_url(config)?;
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
    config: &OssConfig,
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

    match config.kind {
        OssProviderKind::Aliyun => upload_object_with_aliyun_signature(
            client,
            &bytes,
            content_type,
            object_key,
            config,
            base_url,
        ),
        OssProviderKind::R2 => upload_object_with_r2_presigned_url(
            client,
            &bytes,
            content_type,
            object_key,
            config,
            base_url,
        ),
    }
}

fn upload_object_with_aliyun_signature(
    client: &Client,
    bytes: &[u8],
    content_type: &str,
    object_key: &str,
    config: &OssConfig,
    base_url: &Url,
) -> Result<(), String> {
    let date = Utc::now().format("%a, %d %b %Y %H:%M:%S GMT").to_string();
    let canonical_resource = format!(
        "/{}/{}",
        config.bucket.trim(),
        object_key.trim_start_matches('/')
    );
    let string_to_sign = format!("PUT\n\n{content_type}\n{date}\n{canonical_resource}");
    let signature = sign_aliyun_oss_string(&config.access_key_secret, &string_to_sign)?;
    let authorization = format!("OSS {}:{signature}", config.access_key_id.trim());
    let object_url = build_object_url(base_url, config, object_key);

    let response = client
        .put(object_url)
        .header(DATE, date)
        .header(CONTENT_TYPE, content_type)
        .header("Authorization", authorization)
        .body(bytes.to_vec())
        .send()
        .map_err(|error| format!("request failed: {error}"))?;

    ensure_success(response)
}

fn upload_object_with_r2_presigned_url(
    client: &Client,
    bytes: &[u8],
    content_type: &str,
    object_key: &str,
    config: &OssConfig,
    base_url: &Url,
) -> Result<(), String> {
    let expires = config.signed_url_ttl_seconds.clamp(60, 86_400);
    let signed_put_url = build_r2_presigned_url("PUT", object_key, config, base_url, expires)?;

    let response = client
        .put(signed_put_url)
        .header(CONTENT_TYPE, content_type)
        .body(bytes.to_vec())
        .send()
        .map_err(|error| format!("request failed: {error}"))?;

    ensure_success(response)
}

fn ensure_success(response: reqwest::blocking::Response) -> Result<(), String> {
    let status = response.status();
    if !status.is_success() {
        let body = response.text().unwrap_or_default();
        return Err(format!("request failed with {status}: {body}"));
    }
    Ok(())
}

fn build_signed_get_url(
    object_key: &str,
    config: &OssConfig,
    base_url: &Url,
) -> Result<String, String> {
    match config.kind {
        OssProviderKind::Aliyun => build_aliyun_signed_get_url(object_key, config, base_url),
        OssProviderKind::R2 => {
            let expires = config.signed_url_ttl_seconds.clamp(60, 86_400);
            build_r2_presigned_url("GET", object_key, config, base_url, expires)
        }
    }
}

fn build_aliyun_signed_get_url(
    object_key: &str,
    config: &OssConfig,
    base_url: &Url,
) -> Result<String, String> {
    let expires = Utc::now().timestamp() + config.signed_url_ttl_seconds.max(60) as i64;
    let canonical_resource = format!(
        "/{}/{}",
        config.bucket.trim(),
        object_key.trim_start_matches('/')
    );
    let string_to_sign = format!("GET\n\n\n{expires}\n{canonical_resource}");
    let signature = sign_aliyun_oss_string(&config.access_key_secret, &string_to_sign)?;

    let mut url = build_object_url(base_url, config, object_key);
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("OSSAccessKeyId", config.access_key_id.trim());
        query.append_pair("Expires", &expires.to_string());
        query.append_pair("Signature", &signature);
    }
    Ok(url.to_string())
}

fn build_r2_presigned_url(
    method: &str,
    object_key: &str,
    config: &OssConfig,
    base_url: &Url,
    expires_seconds: u64,
) -> Result<String, String> {
    let method = method.trim().to_uppercase();
    if method != "GET" && method != "PUT" {
        return Err(format!("unsupported method for R2 pre-signing: {method}"));
    }

    let object_url = build_object_url(base_url, config, object_key);
    let host = host_header(&object_url)?;
    let now = Utc::now();
    let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
    let date_stamp = now.format("%Y%m%d").to_string();
    let credential_scope = format!(
        "{}/{}/{}/{}",
        date_stamp, AWS_REGION_AUTO, AWS_SERVICE_S3, AWS_REQUEST_TYPE
    );
    let credential = format!("{}/{}", config.access_key_id.trim(), credential_scope);

    let mut params = vec![
        ("X-Amz-Algorithm".to_string(), AWS_ALGORITHM.to_string()),
        ("X-Amz-Credential".to_string(), credential),
        ("X-Amz-Date".to_string(), amz_date.clone()),
        (
            "X-Amz-Expires".to_string(),
            expires_seconds.clamp(60, 86_400).to_string(),
        ),
        ("X-Amz-SignedHeaders".to_string(), "host".to_string()),
    ];

    let canonical_query = canonical_query_string(&params);
    let canonical_uri = aws_encode_path(object_url.path());
    let canonical_request = format!(
        "{}\n{}\n{}\nhost:{}\n\nhost\nUNSIGNED-PAYLOAD",
        method, canonical_uri, canonical_query, host
    );

    let canonical_request_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
    let string_to_sign = format!(
        "{}\n{}\n{}\n{}",
        AWS_ALGORITHM, amz_date, credential_scope, canonical_request_hash
    );

    let signing_key = derive_aws_signing_key(config.access_key_secret.trim(), &date_stamp)?;
    let signature = hex::encode(hmac_sha256(&signing_key, &string_to_sign)?);

    params.push(("X-Amz-Signature".to_string(), signature));
    let final_query = canonical_query_string(&params);

    let mut signed_url = object_url;
    signed_url.set_query(Some(&final_query));
    Ok(signed_url.to_string())
}

fn derive_aws_signing_key(secret: &str, date_stamp: &str) -> Result<Vec<u8>, String> {
    let k_date = hmac_sha256(format!("AWS4{}", secret).as_bytes(), date_stamp)?;
    let k_region = hmac_sha256(&k_date, AWS_REGION_AUTO)?;
    let k_service = hmac_sha256(&k_region, AWS_SERVICE_S3)?;
    hmac_sha256(&k_service, AWS_REQUEST_TYPE)
}

fn hmac_sha256(key: &[u8], data: &str) -> Result<Vec<u8>, String> {
    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|error| format!("failed to initialize HMAC-SHA256: {error}"))?;
    mac.update(data.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

fn canonical_query_string(params: &[(String, String)]) -> String {
    let mut encoded = params
        .iter()
        .map(|(key, value)| (aws_encode(key), aws_encode(value)))
        .collect::<Vec<_>>();
    encoded.sort_by(|left, right| left.cmp(right));
    encoded
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

fn aws_encode(value: &str) -> String {
    utf8_percent_encode(value, AWS_ENCODE_SET).to_string()
}

fn aws_encode_path(path: &str) -> String {
    let encoded = path
        .split('/')
        .map(aws_encode)
        .collect::<Vec<_>>()
        .join("/");
    if encoded.is_empty() {
        "/".to_string()
    } else {
        encoded
    }
}

fn host_header(url: &Url) -> Result<String, String> {
    let host = url
        .host_str()
        .ok_or_else(|| "OSS endpoint must contain host".to_string())?;
    let port = url.port();
    let scheme = url.scheme();
    let has_non_default_port = match (scheme, port) {
        ("http", Some(80)) | ("https", Some(443)) | (_, None) => false,
        (_, Some(_)) => true,
    };
    if has_non_default_port {
        Ok(format!("{host}:{}", url.port().unwrap_or_default()))
    } else {
        Ok(host.to_string())
    }
}

fn build_base_url(config: &OssConfig) -> Result<Url, String> {
    let endpoint = config.endpoint.trim();
    if endpoint.is_empty() {
        return Err("OSS endpoint is empty".to_string());
    }
    let bucket = config.bucket.trim();
    if bucket.is_empty() {
        return Err("OSS bucket is empty".to_string());
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

    if config.kind == OssProviderKind::Aliyun {
        let host = base_url.host_str().unwrap_or_default();
        let bucket_host = if host.starts_with(&format!("{bucket}.")) {
            host.to_string()
        } else {
            format!("{bucket}.{host}")
        };
        base_url
            .set_host(Some(&bucket_host))
            .map_err(|_| "invalid OSS bucket or endpoint host".to_string())?;
    }

    base_url.set_path("/");
    base_url.set_query(None);
    base_url.set_fragment(None);
    Ok(base_url)
}

fn build_object_url(base_url: &Url, config: &OssConfig, object_key: &str) -> Url {
    let mut url = base_url.clone();
    let normalized_key = object_key.trim().trim_start_matches('/');
    match config.kind {
        OssProviderKind::Aliyun => {
            url.set_path(&format!("/{normalized_key}"));
        }
        OssProviderKind::R2 => {
            url.set_path(&format!("/{}/{}", config.bucket.trim(), normalized_key));
        }
    }
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

fn sign_aliyun_oss_string(secret: &str, content: &str) -> Result<String, String> {
    let mut mac = HmacSha1::new_from_slice(secret.as_bytes())
        .map_err(|error| format!("failed to initialize HMAC-SHA1: {error}"))?;
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
    use super::{aws_encode_path, build_base_url, build_object_key, OssConfig, OssProviderKind};

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
    fn build_base_url_prefixes_bucket_for_aliyun() {
        let config = OssConfig {
            kind: OssProviderKind::Aliyun,
            access_key_id: "ak".to_string(),
            access_key_secret: "sk".to_string(),
            endpoint: "https://oss-cn-beijing.aliyuncs.com".to_string(),
            bucket: "my-bucket".to_string(),
            path_prefix: None,
            signed_url_ttl_seconds: 1800,
        };
        let url = build_base_url(&config).expect("url should be valid");
        assert_eq!(
            url.as_str(),
            "https://my-bucket.oss-cn-beijing.aliyuncs.com/"
        );
    }

    #[test]
    fn build_base_url_keeps_r2_endpoint_host() {
        let config = OssConfig {
            kind: OssProviderKind::R2,
            access_key_id: "ak".to_string(),
            access_key_secret: "sk".to_string(),
            endpoint: "https://1234567890abcdef.r2.cloudflarestorage.com".to_string(),
            bucket: "my-bucket".to_string(),
            path_prefix: None,
            signed_url_ttl_seconds: 1800,
        };
        let url = build_base_url(&config).expect("url should be valid");
        assert_eq!(
            url.as_str(),
            "https://1234567890abcdef.r2.cloudflarestorage.com/"
        );
    }

    #[test]
    fn aws_path_encoding_preserves_slashes() {
        assert_eq!(
            aws_encode_path("/bucket/my file/segment 1.wav"),
            "/bucket/my%20file/segment%201.wav"
        );
    }
}
