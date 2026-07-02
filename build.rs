fn main() {
    println!("cargo:rerun-if-changed=assets/app.ico");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "1.0.0".to_owned());

    let mut resource = winresource::WindowsResource::new();
    resource.set_icon("assets/app.ico");
    resource.set("ProductName", "MySQL Tray Controller");
    resource.set(
        "FileDescription",
        "Monitor and control local MySQL and MariaDB Windows services",
    );
    resource.set("CompanyName", "Mohsen Manoochehr");
    resource.set("LegalCopyright", "Copyright (c) 2026 Mohsen Manoochehr");
    resource.set("OriginalFilename", "mysql-tray-controller.exe");
    resource.set("FileVersion", &version);
    resource.set("ProductVersion", &version);

    if let Err(error) = resource.compile() {
        panic!("Could not compile Windows resources: {error}");
    }
}
