// Иконка и сведения о программе внутри .exe (Windows).
fn main() {
    println!("cargo:rerun-if-changed=../app.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../app.ico");
        res.set("ProductName", "Very Good Downloader");
        res.set("FileDescription", "Very Good Downloader");
        if let Err(e) = res.compile() {
            println!("cargo:warning=иконка не встроена: {e}");
        }
    }
}
