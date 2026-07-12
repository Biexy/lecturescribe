import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";
import "./styles/components.css";
import "./styles/dialogs.css";
import "./styles/responsive.css";

const container = document.getElementById("app");

if (!container) {
  throw new Error("LectureScribe root element was not found.");
}

createRoot(container).render(<App />);
