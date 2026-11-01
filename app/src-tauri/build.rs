fn main() {
    // 产品名只在 tauri.conf.json 里写一次；这里读出来变成编译期常量 PRODUCT_NAME，
    // 窗口标题、缺组件时的提示、HTTP User-Agent 都用它，免得各处再写死一份。
    println!("cargo:rerun-if-changed=tauri.conf.json");
    let raw = std::fs::read_to_string("tauri.conf.json").expect("读不到 tauri.conf.json");
    let conf: serde_json::Value = serde_json::from_str(&raw).expect("tauri.conf.json 不是合法 JSON");
    let product = conf
        .get("productName")
        .and_then(|v| v.as_str())
        .expect("tauri.conf.json 里必须有 productName");
    println!("cargo:rustc-env=PRODUCT_NAME={product}");

    tauri_build::build()
}