import { App } from "./App";
import { createRoot } from "./runtime/dom-renderer";
import "./styles.css";

const container = document.getElementById("app");

if (!container) {
  throw new Error("LectureScribe root element was not found.");
}

createRoot(container).render(<App />);
