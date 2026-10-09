//! Gives the Windows executable its icon, so Explorer and the taskbar show it. Nothing to do on
//! other systems.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/app-icon.ico");
    #[cfg(windows)]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/app-icon.ico");
        if let Err(error) = resource.compile() {
            println!("cargo:warning=could not add the icon to the executable: {error}");
        }
    }
}
