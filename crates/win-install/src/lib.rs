//! Brocode's per-user Windows install, shared by the setup (`apps/installer`) and the
//! app, which uninstalls itself (`--uninstall`) and refreshes its uninstall entry after an
//! update (`--apply-update`).
//!
//! ```text
//! %LOCALAPPDATA%\Programs\OpenCode Rune\               the install root (the updater swaps it whole)
//!   opencode-desktop.exe                               icon, version and manifest resources inside
//!   install.json                                       the setup's receipt: the updater replaces only this install
//! Start menu\Programs\Brocode.lnk              AppUserModelID set, so a pin and the
//! Desktop\Brocode.lnk (optional)               window share one taskbar button
//! HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\<app id>    Apps & features
//! HKCU\Software\Classes\<app scheme>                   the app's links and toast clicks (never `opencode`)
//! HKCU\Software\Classes\AppUserModelId\<app id>       the toasts' name and icon
//! ```
//!
//! Nothing needs an administrator: every file is under the user's profile and every key under
//! `HKEY_CURRENT_USER`. The root's path never changes, so the shortcuts, the entry and the
//! schemes stay valid across updates. [`Product`] is the identity each build compiles in, so
//! the setup and the app it installs agree on every name.

mod product;
#[cfg(feature = "resources")]
pub mod resources;
#[cfg(target_os = "windows")]
mod win;

pub use product::{EXE, HOMEPAGE, OPENCODE_SCHEME, PUBLISHER, Product, default_name, side_by_side};
#[cfg(target_os = "windows")]
pub use win::{Identity, RECEIPT, close_running, has_receipt, installed_by_setup, write_receipt, default_root, installed_root, other_installs, refresh, register, remove_dir, repair_scheme, running_under, settle_identity, unregister};
#[cfg(target_os = "windows")]
pub use win::folders::{documents, local_app_data, protected_folders, roaming_app_data};
