use tauri::webview::PageLoadEvent;
use tauri::{Webview, WebviewWindow};

#[cfg(all(debug_assertions, target_os = "macos"))]
mod mac {
    use std::path::{Path, PathBuf};

    use block2::RcBlock;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
    use objc2_foundation::{NSDictionary, NSError};
    use objc2_web_kit::WKWebView;
    use tauri::WebviewWindow;

    fn save(image: *mut NSImage, path: &Path) {
        let Some(image) = (unsafe { image.as_ref() }) else {
            return;
        };
        let Some(tiff) = image.TIFFRepresentation() else {
            return;
        };
        let Some(bitmap) = NSBitmapImageRep::imageRepWithData(&tiff) else {
            return;
        };
        let Some(png) = (unsafe {
            bitmap.representationUsingType_properties(
                NSBitmapImageFileType::PNG,
                &NSDictionary::new(),
            )
        }) else {
            return;
        };
        let _ = std::fs::write(path, png.to_vec());
    }

    pub fn shot(window: &WebviewWindow, path: PathBuf) -> Result<(), String> {
        window
            .with_webview(move |webview| {
                let Some(view) = (unsafe { webview.inner().cast::<WKWebView>().as_ref() }) else {
                    return;
                };
                let done =
                    RcBlock::new(move |image: *mut NSImage, _: *mut NSError| save(image, &path));
                unsafe { view.takeSnapshotWithConfiguration_completionHandler(None, &done) };
            })
            .map_err(|error| error.to_string())
    }
}

#[tauri::command]
#[specta::specta]
pub fn shot(window: WebviewWindow, name: String) -> Result<String, String> {
    let folder = std::env::temp_dir().join("sombre-shots");
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    let path = folder.join(format!("{name}.png"));
    let shown = path.display().to_string();
    #[cfg(all(debug_assertions, target_os = "macos"))]
    {
        mac::shot(&window, path)?;
        Ok(shown)
    }
    #[cfg(not(all(debug_assertions, target_os = "macos")))]
    {
        let _ = (window, shown);
        Err("snapshots only work in debug builds on a mac".to_string())
    }
}

pub fn script(webview: &Webview, event: PageLoadEvent) {
    if !cfg!(debug_assertions) || event != PageLoadEvent::Finished {
        return;
    }
    if let Some(script) =
        std::env::var_os("SOMBRE_SHOTS").and_then(|path| std::fs::read_to_string(path).ok())
    {
        let _ = webview.eval(script);
    }
}
