use std::path::PathBuf;

pub fn pick_folder() -> Option<PathBuf> {
    pick(true, false)
}
pub fn pick_image() -> Option<PathBuf> {
    pick(false, false)
}
pub fn pick_dll() -> Option<PathBuf> {
    pick(false, true)
}

#[cfg(windows)]
fn pick(folder: bool, dll: bool) -> Option<PathBuf> {
    // A dedicated STA keeps COM ownership independent of the window/event loop.
    std::thread::spawn(move || unsafe {
        use windows::{
            core::w,
            Win32::{
                Foundation::HWND,
                System::Com::{
                    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
                    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
                },
                UI::Shell::{
                    Common::COMDLG_FILTERSPEC, FileOpenDialog, IFileOpenDialog, FOS_FILEMUSTEXIST,
                    FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
                },
            },
        };
        CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().ok()?;
        let result = (|| -> windows::core::Result<PathBuf> {
            let dialog: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
            let options = dialog.GetOptions()?
                | FOS_FORCEFILESYSTEM
                | if folder {
                    FOS_PICKFOLDERS
                } else {
                    FOS_FILEMUSTEXIST
                };
            dialog.SetOptions(options)?;
            if dll {
                dialog.SetFileTypes(&[COMDLG_FILTERSPEC {
                    pszName: w!("DLL"),
                    pszSpec: w!("*.dll"),
                }])?;
            } else if !folder {
                dialog.SetFileTypes(&[COMDLG_FILTERSPEC {
                    pszName: w!("PNG / JPEG"),
                    pszSpec: w!("*.png;*.jpg;*.jpeg"),
                }])?;
            }
            dialog.Show(HWND::default())?;
            let name = dialog.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
            let path = name.to_string();
            CoTaskMemFree(Some(name.0.cast()));
            Ok(PathBuf::from(path?))
        })();
        CoUninitialize();
        result.ok()
    })
    .join()
    .ok()
    .flatten()
}
#[cfg(not(windows))]
fn pick(_: bool, _: bool) -> Option<PathBuf> {
    None
}
