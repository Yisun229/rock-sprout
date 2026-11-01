import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { APP_TITLE } from "./brand";
import { AppStoreProvider } from "./store/appStore";
import "./styles.css";

// 窗口标题由 Rust 用同一个产品名设；这里补文档标题，两边保持一致
document.title = APP_TITLE;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <AppStoreProvider>
      <App />
    </AppStoreProvider>
  </React.StrictMode>
);
