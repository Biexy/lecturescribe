export type IconName =
  | "alert"
  | "bug"
  | "check"
  | "chevron-down"
  | "chevron-right"
  | "clock"
  | "copy"
  | "download"
  | "external"
  | "eye"
  | "file"
  | "file-audio"
  | "file-up"
  | "filter"
  | "folder"
  | "help"
  | "history"
  | "info"
  | "key"
  | "layers"
  | "link"
  | "list"
  | "moon"
  | "more"
  | "pause"
  | "play"
  | "refresh"
  | "search"
  | "settings"
  | "shield"
  | "square"
  | "sun"
  | "trash"
  | "undo"
  | "video"
  | "wrench"
  | "x";

const paths: Record<IconName, string[]> = {
  alert: ["M10.3 2.9 1.8 17a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 2.9a2 2 0 0 0-3.4 0Z", "M12 9v4", "M12 17h.01"],
  bug: ["M8 2v2", "M16 2v2", "M9 9h6", "M10 4h4a4 4 0 0 1 4 4v7a6 6 0 0 1-12 0V8a4 4 0 0 1 4-4Z", "M3 13h3", "M18 13h3", "M3 19h4", "M17 19h4"],
  check: ["M20 6 9 17l-5-5"],
  "chevron-down": ["m6 9 6 6 6-6"],
  "chevron-right": ["m9 18 6-6-6-6"],
  clock: ["M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Z", "M12 6v6l4 2"],
  copy: ["M8 8h11a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2v-9a2 2 0 0 1 2-2Z", "M16 8V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h1"],
  download: ["M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4", "m7 10 5 5 5-5", "M12 15V3"],
  external: ["M15 3h6v6", "M10 14 21 3", "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"],
  eye: ["M2.1 12a10.9 10.9 0 0 1 19.8 0 10.9 10.9 0 0 1-19.8 0Z", "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z"],
  file: ["M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z", "M14 2v6h6", "M8 13h8", "M8 17h6"],
  "file-audio": ["M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z", "M14 2v6h6", "M9 18v-6l5-1v6", "M9 18a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0Z", "M14 17a1.5 1.5 0 1 1-3 0 1.5 1.5 0 0 1 3 0Z"],
  "file-up": ["M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z", "M14 2v6h6", "M12 18v-6", "m9 15 3-3 3 3"],
  filter: ["M4 5h16", "M7 12h10", "M10 19h4"],
  folder: ["M3 6h5l2 2h11v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2Z", "M3 10h18"],
  help: ["M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Z", "M9.1 9a3 3 0 1 1 5.8 1c0 2-3 2-3 4", "M12 18h.01"],
  history: ["M3 12a9 9 0 1 0 3-6.7L3 8", "M3 3v5h5", "M12 7v5l3 2"],
  info: ["M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20Z", "M12 10v6", "M12 7h.01"],
  key: ["M21 2l-2 2", "M7.6 14.4a5 5 0 1 1 7-7 5 5 0 0 1-7 7Z", "m14 5-3 3", "m16 7 2 2"],
  layers: ["m12 2 9 5-9 5-9-5Z", "m3 12 9 5 9-5", "m3 17 9 5 9-5"],
  link: ["M10 13a5 5 0 0 0 7.1.1l2-2a5 5 0 0 0-7.1-7.1l-1.1 1.1", "M14 11a5 5 0 0 0-7.1-.1l-2 2A5 5 0 0 0 12 20l1.1-1.1"],
  list: ["M8 6h13", "M8 12h13", "M8 18h13", "M3 6h.01", "M3 12h.01", "M3 18h.01"],
  moon: ["M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"],
  more: ["M12 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2Z", "M19 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2Z", "M5 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2Z"],
  pause: ["M8 5v14", "M16 5v14"],
  play: ["m6 3 14 9-14 9Z"],
  refresh: ["M20 11a8 8 0 1 0-2.3 5.7", "M20 4v7h-7"],
  search: ["M21 21l-4.3-4.3", "M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16Z"],
  settings: ["M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7Z", "M19.4 15a1.7 1.7 0 0 0 .3 1.9l.1.1-2 3.5-.2-.1a1.7 1.7 0 0 0-1.8-.2l-.6.4a1.7 1.7 0 0 0-.8 1.5v.2h-4v-.2a1.7 1.7 0 0 0-.8-1.5l-.6-.4a1.7 1.7 0 0 0-1.8.2l-.2.1-2-3.5.1-.1a1.7 1.7 0 0 0 .3-1.9l-.3-.7a1.7 1.7 0 0 0-1.5-1H3V9h.2a1.7 1.7 0 0 0 1.5-1l.3-.7a1.7 1.7 0 0 0-.3-1.9l-.1-.1 2-3.5.2.1a1.7 1.7 0 0 0 1.8.2l.6-.4A1.7 1.7 0 0 0 10 0h4a1.7 1.7 0 0 0 .8 1.5l.6.4a1.7 1.7 0 0 0 1.8-.2l.2-.1 2 3.5-.1.1a1.7 1.7 0 0 0-.3 1.9l.3.7a1.7 1.7 0 0 0 1.5 1h.2v4h-.2a1.7 1.7 0 0 0-1.5 1Z"],
  shield: ["M20 13c0 5-3.5 7.5-8 9-4.5-1.5-8-4-8-9V5l8-3 8 3Z", "m9 12 2 2 4-4"],
  square: ["M5 5h14v14H5Z"],
  sun: ["M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6Z", "M12 2v2", "M12 20v2", "m4.9 4.9 1.4 1.4", "m17.7 17.7 1.4 1.4", "M2 12h2", "M20 12h2", "m6.3 17.7-1.4 1.4", "m19.1 4.9-1.4 1.4"],
  trash: ["M3 6h18", "M8 6V4h8v2", "M19 6l-1 15H6L5 6", "M10 11v5", "M14 11v5"],
  undo: ["M9 14 4 9l5-5", "M4 9h10a6 6 0 0 1 0 12h-2"],
  video: ["M15 10 20 7v10l-5-3", "M4 6h11v12H4a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2Z"],
  wrench: ["M14.7 6.3a4 4 0 0 0-5-5L7 4l3 3-3 3-3-3-2.7 2.7a4 4 0 0 0 5 5L17 4l3 3Z", "m5 19 5-5"],
  x: ["M18 6 6 18", "M6 6l12 12"],
};

export function Icon({ name, size = 18, className = "" }: { name: IconName; size?: number; className?: string }) {
  return (
    <svg
      aria-hidden="true"
      className={`icon ${className}`}
      fill="none"
      height={size}
      viewBox="0 0 24 24"
      width={size}
      stroke="currentColor"
      strokeLinecap="round"
      strokeLinejoin="round"
      strokeWidth="2"
    >
      {paths[name].map((path, index) => <path d={path} key={`${name}-${index}`} />)}
    </svg>
  );
}
