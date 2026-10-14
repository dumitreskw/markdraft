//! The `mdasset` URI scheme: serves local images referenced by documents.
//!
//! The WebView has no file-system access, so the renderer rewrites local
//! image paths to `mdasset` URLs and this handler reads them. Only image
//! file types are served.

use std::path::{Path, PathBuf};

use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use tauri::http::{Request, Response, StatusCode, header};
use tauri::{Runtime, UriSchemeContext, UriSchemeResponder};

pub const SCHEME: &str = "mdasset";

/// Images larger than this are not served to the preview.
const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;

/// URL under which the WebView can load `path`.
pub fn asset_url(path: &Path) -> String {
    let encoded = utf8_percent_encode(&path.to_string_lossy(), NON_ALPHANUMERIC).to_string();
    // WebView2 (Windows) only supports custom schemes as http://<scheme>.localhost.
    if cfg!(windows) {
        format!("http://{SCHEME}.localhost/{encoded}")
    } else {
        format!("{SCHEME}://localhost/{encoded}")
    }
}

/// The file a request URL points at.
fn path_from_uri(uri_path: &str) -> PathBuf {
    let trimmed = uri_path.trim_start_matches('/');
    PathBuf::from(percent_decode_str(trimmed).decode_utf8_lossy().into_owned())
}

pub fn mime_for(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    Some(match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        _ => return None,
    })
}

pub fn handle<R: Runtime>(
    _ctx: UriSchemeContext<'_, R>,
    request: Request<Vec<u8>>,
    responder: UriSchemeResponder,
) {
    let path = path_from_uri(request.uri().path());
    std::thread::spawn(move || {
        responder.respond(serve(&path));
    });
}

fn serve(path: &Path) -> Response<Vec<u8>> {
    let Some(mime) = mime_for(path) else {
        return status(StatusCode::FORBIDDEN);
    };
    match std::fs::metadata(path) {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_IMAGE_BYTES => {}
        Ok(_) => return status(StatusCode::FORBIDDEN),
        Err(_) => return status(StatusCode::NOT_FOUND),
    }
    match std::fs::read(path) {
        Ok(bytes) => Response::builder()
            .header(header::CONTENT_TYPE, mime)
            // SVGs must never run script, even when opened directly.
            .header(
                header::CONTENT_SECURITY_POLICY,
                "default-src 'none'; style-src 'unsafe-inline'",
            )
            .header(header::CACHE_CONTROL, "no-cache")
            .body(bytes)
            .unwrap_or_else(|_| status(StatusCode::INTERNAL_SERVER_ERROR)),
        Err(_) => status(StatusCode::NOT_FOUND),
    }
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(code)
        .body(Vec::new())
        .expect("static response is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_round_trip_paths_with_spaces_and_unicode() {
        let path = std::env::temp_dir().join("my pics").join("café #1.png");
        let url = asset_url(&path);
        assert!(!url.contains(' '));
        let uri_path = url
            .split_once(".localhost")
            .or_else(|| url.split_once("localhost"))
            .unwrap()
            .1;
        assert_eq!(path_from_uri(uri_path), path);
    }

    #[test]
    fn only_images_are_served() {
        let dir = tempfile::tempdir().unwrap();
        let secret = dir.path().join("secret.txt");
        std::fs::write(&secret, "x").unwrap();
        assert_eq!(serve(&secret).status(), StatusCode::FORBIDDEN);
        let img = dir.path().join("a.PNG");
        std::fs::write(&img, [137, 80, 78, 71]).unwrap();
        let ok = serve(&img);
        assert_eq!(ok.status(), StatusCode::OK);
        assert_eq!(ok.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(
            serve(&dir.path().join("missing.png")).status(),
            StatusCode::NOT_FOUND
        );
    }
}
