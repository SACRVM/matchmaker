//! On Windows the program carries its version in its resources: Husky
//! reads it there (`FileVersionInfo`) to tell whether a release is newer.

fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set("FileDescription", "Game codes and a relay for peer-to-peer games");
        res.set("ProductName", "matchmaker");
        res.set("LegalCopyright", "Copyright (c) 2026 matchmaker contributors");
        res.compile().expect("the program's resources");
    }
}
