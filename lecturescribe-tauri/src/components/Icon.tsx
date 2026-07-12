import {
  AlertTriangle,
  AudioLines,
  Bug,
  Check,
  ChevronDown,
  ChevronRight,
  CircleHelp,
  Clock,
  Copy,
  Download,
  Ellipsis,
  ExternalLink,
  Eye,
  FileAudio,
  FileText,
  FileUp,
  Filter,
  Folder,
  History,
  Info,
  KeyRound,
  Layers,
  Link,
  List,
  Moon,
  Pause,
  Play,
  RefreshCw,
  Search,
  Settings,
  ShieldCheck,
  Square,
  Sun,
  Trash2,
  Undo2,
  Video,
  Wrench,
  X,
  type LucideIcon,
} from "lucide-react";

export type IconName =
  | "alert"
  | "audio-lines"
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

const icons: Record<IconName, LucideIcon> = {
  alert: AlertTriangle,
  "audio-lines": AudioLines,
  bug: Bug,
  check: Check,
  "chevron-down": ChevronDown,
  "chevron-right": ChevronRight,
  clock: Clock,
  copy: Copy,
  download: Download,
  external: ExternalLink,
  eye: Eye,
  file: FileText,
  "file-audio": FileAudio,
  "file-up": FileUp,
  filter: Filter,
  folder: Folder,
  help: CircleHelp,
  history: History,
  info: Info,
  key: KeyRound,
  layers: Layers,
  link: Link,
  list: List,
  moon: Moon,
  more: Ellipsis,
  pause: Pause,
  play: Play,
  refresh: RefreshCw,
  search: Search,
  settings: Settings,
  shield: ShieldCheck,
  square: Square,
  sun: Sun,
  trash: Trash2,
  undo: Undo2,
  video: Video,
  wrench: Wrench,
  x: X,
};

export function Icon({ name, size = 18, className = "" }: { name: IconName; size?: number; className?: string }) {
  const LucideIcon = icons[name];
  return <LucideIcon aria-hidden="true" className={`icon ${className}`} color="currentColor" size={size} />;
}
