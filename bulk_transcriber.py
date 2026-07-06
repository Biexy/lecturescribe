import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import parse_qs, urlparse


BASE_DIR = Path(__file__).resolve().parent
DEFAULT_MODEL = "gemini-3.1-flash-lite"
CHUNK_CACHE_VERSION = "audio-v2"
MEDIA_EXTENSIONS = {
    ".mp3",
    ".m4a",
    ".mp4",
    ".webm",
    ".wav",
    ".aac",
    ".flac",
    ".ogg",
    ".opus",
    ".mov",
    ".mkv",
}
SKIP_MEDIA_FOLDER_NAMES = {
    ".git",
    "__pycache__",
    "node_modules",
    "target",
    "dist",
    "_work",
    "audio_chunks",
    "chunk_text",
}


@dataclass
class MediaItem:
    number: int
    title: str
    source: str
    media_path: Path
    item_id: str


def parse_args():
    parser = argparse.ArgumentParser(
        description="Download and chunk-transcribe many lecture videos with Gemini."
    )
    parser.add_argument(
        "inputs",
        nargs="*",
        help="Link text files, media files, or folders. Defaults to links.txt if it exists.",
    )
    parser.add_argument("--download-dir", default="downloads", help="Folder for downloaded media.")
    parser.add_argument("--output-dir", default=str(Path("Transcripts") / "organized"), help="Final transcript folder.")
    parser.add_argument("--work-dir", default=str(Path("Transcripts") / "_work"), help="Chunk/cache folder.")
    parser.add_argument("--skip-download", action="store_true", help="Use existing media only.")
    parser.add_argument("--dry-run", action="store_true", help="Show planned media items without downloading or transcribing.")
    parser.add_argument("--dry-run-json", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--chunk-minutes", type=int, default=2, help="Audio chunk size in minutes.")
    parser.add_argument("--start-at", type=int, default=1, help="1-based item number to start at.")
    parser.add_argument("--end-at", type=int, help="1-based item number to stop at.")
    parser.add_argument("--force", action="store_true", help="Regenerate final transcript files.")
    parser.add_argument("--model", help="Gemini model name. Defaults to GEMINI_MODEL or gemini-3.1-flash-lite.")
    parser.add_argument("--max-api-retries", type=int, default=100, help="Retries per item for temporary API errors.")
    parser.add_argument("--request-delay-seconds", type=float, default=5, help="Delay after each new chunk request.")
    parser.add_argument(
        "--cookies-from-browser",
        default=os.environ.get("YT_DLP_COOKIES_FROM_BROWSER", ""),
        help="Browser name for yt-dlp cookies, for example chrome, edge, or firefox.",
    )
    parser.add_argument(
        "--cookies-file",
        default=os.environ.get("YT_DLP_COOKIE_FILE", ""),
        help="Cookie file path for yt-dlp private or restricted links.",
    )
    return parser.parse_args()


def resolve_path(raw_path):
    path = Path(raw_path)
    if not path.is_absolute():
        path = BASE_DIR / path
    return path


def load_local_env():
    env_path = BASE_DIR / ".env"
    if not env_path.exists():
        return

    for raw_line in env_path.read_text(encoding="utf-8-sig").splitlines():
        line = raw_line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip().strip('"').strip("'")
        if key and key not in os.environ:
            os.environ[key] = value


def require_api_key():
    load_local_env()
    api_key = os.environ.get("GEMINI_API_KEY") or os.environ.get("GOOGLE_API_KEY")
    if not api_key or api_key == "put-your-gemini-api-key-here":
        raise RuntimeError("Set GEMINI_API_KEY in .env or in your shell environment.")
    return api_key


def load_genai_modules():
    try:
        from google import genai as genai_module
        from google.genai import errors as errors_module
        from google.genai import types as types_module
    except ImportError as exc:
        raise RuntimeError("google-genai is not installed. Run setup.ps1 first.") from exc
    return genai_module, errors_module, types_module


def safe_filename(value):
    value = re.sub(r'[<>:"/\\|?*]+', " ", value)
    value = re.sub(r"\s+", " ", value).strip(" .")
    return value or "Untitled"


def seconds_to_stamp(seconds):
    seconds = int(seconds)
    hours, remainder = divmod(seconds, 3600)
    minutes, seconds = divmod(remainder, 60)
    if hours:
        return f"{hours:02d}:{minutes:02d}:{seconds:02d}"
    return f"{minutes:02d}:{seconds:02d}"


def is_google_drive_url(url):
    try:
        host = urlparse(url).netloc.lower()
    except ValueError:
        return False
    return host.endswith("drive.google.com") or host.endswith("docs.google.com")


def extract_drive_file_id(url):
    parsed = urlparse(url)
    query = parse_qs(parsed.query)
    if query.get("id"):
        return query["id"][0]

    parts = [part for part in parsed.path.split("/") if part]
    for index, part in enumerate(parts):
        if part == "d" and index + 1 < len(parts):
            return parts[index + 1]
    return None


def normalize_google_drive_url(url):
    if not is_google_drive_url(url):
        return url
    file_id = extract_drive_file_id(url)
    if not file_id:
        return url
    return f"https://drive.google.com/file/d/{file_id}/view"


def extract_bracket_id(path):
    matches = re.findall(r"\[([^\]]+)\]", path.stem)
    return matches[-1] if matches else None


def clean_title(path):
    title = re.sub(r"\s*\[[^\]]+\]\s*$", "", path.stem)
    title = re.sub(r"\.mp4$", "", title, flags=re.IGNORECASE)
    title = title.replace("_", " ").strip()
    return re.sub(r"\s+", " ", title)


def read_link_file(path):
    return extract_urls(path.read_text(encoding="utf-8-sig"))


def extract_urls(text):
    links = []
    seen = set()
    for match in re.finditer(r"https?://[^\s<>'\"]+", text):
        url = match.group(0).strip().strip("<>()[]{}\"'")
        url = url.rstrip(".,;)]}")
        if url and url not in seen:
            links.append(url)
            seen.add(url)
    return links


def collect_links(link_files):
    links = []
    seen = set()
    for link_file in link_files:
        for url in read_link_file(link_file):
            if url not in seen:
                links.append(url)
                seen.add(url)
    return links


def split_inputs(inputs):
    if not inputs and (BASE_DIR / "links.txt").exists():
        inputs = ["links.txt"]

    link_files = []
    media_sources = []
    for raw_input in inputs:
        path = resolve_path(raw_input)
        if path.is_file() and path.suffix.lower() == ".txt":
            link_files.append(path)
        elif path.is_file() and path.suffix.lower() in MEDIA_EXTENSIONS:
            media_sources.append(path)
        elif path.is_dir():
            media_sources.extend(iter_media_files(path))
        else:
            print(f"[!] Skipping unknown input: {path}", flush=True)

    return link_files, media_sources


def iter_media_files(folder):
    if not folder.exists():
        return []
    return [
        path
        for path in sorted(folder.rglob("*"))
        if path.is_file()
        and path.suffix.lower() in MEDIA_EXTENSIONS
        and not is_inside_skipped_media_folder(path)
    ]


def is_inside_skipped_media_folder(path):
    return any(part.lower() in SKIP_MEDIA_FOLDER_NAMES for part in path.parts)


def find_yt_dlp_command():
    bundled = BASE_DIR / "yt-dlp.exe"
    if bundled.exists():
        return [str(bundled)]

    script_name = "yt-dlp.exe" if os.name == "nt" else "yt-dlp"
    venv_script = Path(sys.executable).resolve().parent / script_name
    if venv_script.exists():
        return [str(venv_script)]

    path = shutil.which("yt-dlp")
    if path:
        return [path]

    try:
        import yt_dlp  # noqa: F401
    except ImportError:
        pass
    else:
        return [sys.executable, "-m", "yt_dlp"]

    raise RuntimeError("yt-dlp was not found. Run setup.ps1 or install yt-dlp.")


def yt_dlp_cookie_args(cookies_from_browser="", cookies_file=""):
    browser = cookies_from_browser.strip()
    if browser:
        return ["--cookies-from-browser", browser]

    cookie_file = cookies_file.strip()
    if cookie_file:
        return ["--cookies", cookie_file]

    return []


def download_links(link_files, download_dir, cookies_from_browser="", cookies_file=""):
    links = collect_links(link_files)
    if not links:
        return []

    yt_dlp = find_yt_dlp_command()
    download_dir.mkdir(parents=True, exist_ok=True)
    output_template = str(download_dir / "%(title).200B [%(id)s].%(ext)s")

    print(f"[+] Downloading {len(links)} URL(s) to {download_dir}", flush=True)
    downloaded_sources = []
    for index, original_url in enumerate(links, start=1):
        url = normalize_google_drive_url(original_url)
        print(f"[+] Download {index}/{len(links)}", flush=True)
        command = [
            *yt_dlp,
            "--ignore-errors",
            "--no-overwrites",
            "--extract-audio",
            "--audio-format",
            "mp3",
            "--audio-quality",
            "0",
            "--output",
            output_template,
            *yt_dlp_cookie_args(cookies_from_browser, cookies_file),
            url,
        ]
        result = subprocess.run(command, cwd=BASE_DIR)
        if result.returncode == 0:
            downloaded_sources.append(original_url)
        else:
            print(f"[X] Download failed: {original_url}", flush=True)
            if is_google_drive_url(url):
                print(
                    "[~] For private Drive files, try --cookies-from-browser chrome "
                    "or set YT_DLP_COOKIES_FROM_BROWSER=chrome.",
                    flush=True,
                )

    return downloaded_sources


def build_media_items(link_files, media_sources, download_dir):
    items = []
    used_paths = set()
    downloaded_media = iter_media_files(download_dir)
    media_by_id = {
        file_id: path
        for path in downloaded_media
        for file_id in [extract_bracket_id(path)]
        if file_id
    }

    for link_file in link_files:
        for url in read_link_file(link_file):
            file_id = extract_drive_file_id(url) if is_google_drive_url(url) else None
            media_path = media_by_id.get(file_id) if file_id else None
            if not media_path:
                continue
            used_paths.add(media_path.resolve())
            items.append(
                MediaItem(
                    number=len(items) + 1,
                    title=clean_title(media_path),
                    source=url,
                    media_path=media_path,
                    item_id=file_id or extract_bracket_id(media_path) or str(len(items) + 1),
                )
            )

    for media_path in [*media_sources, *downloaded_media]:
        resolved = media_path.resolve()
        if resolved in used_paths:
            continue
        used_paths.add(resolved)
        items.append(
            MediaItem(
                number=len(items) + 1,
                title=clean_title(media_path),
                source=str(media_path),
                media_path=media_path,
                item_id=extract_bracket_id(media_path) or f"local-{len(items) + 1}",
            )
        )

    return items


def split_audio(media_path, item_chunk_dir, chunk_seconds):
    item_chunk_dir.mkdir(parents=True, exist_ok=True)
    existing = sorted(item_chunk_dir.glob("chunk_*.mp3"))
    if existing:
        return existing

    pattern = str(item_chunk_dir / "chunk_%03d.mp3")
    command = [
        "ffmpeg",
        "-hide_banner",
        "-loglevel",
        "error",
        "-i",
        str(media_path),
        "-f",
        "segment",
        "-segment_time",
        str(chunk_seconds),
        "-reset_timestamps",
        "1",
        "-map",
        "0:a:0",
        "-vn",
        "-ac",
        "1",
        "-ar",
        "16000",
        "-codec:a",
        "libmp3lame",
        "-b:a",
        "64k",
        pattern,
    ]
    subprocess.run(command, check=True)
    return sorted(item_chunk_dir.glob("chunk_*.mp3"))


def repetition_report(text):
    words = re.findall(r"[\w\u0600-\u06FF]+", text, flags=re.UNICODE)
    if not words:
        return {"words": 0, "max_run": 0, "max_5gram": 0}

    max_run = 1
    current_run = 1
    for previous, current in zip(words, words[1:]):
        if previous == current:
            current_run += 1
            max_run = max(max_run, current_run)
        else:
            current_run = 1

    fivegram_counts = {}
    for index in range(max(0, len(words) - 4)):
        gram = tuple(words[index : index + 5])
        fivegram_counts[gram] = fivegram_counts.get(gram, 0) + 1

    return {
        "words": len(words),
        "max_run": max_run,
        "max_5gram": max(fivegram_counts.values(), default=0),
    }


def looks_repetitive(text):
    report = repetition_report(text)
    return report["max_run"] >= 25 or report["max_5gram"] >= 25


def trim_looping_text(text):
    token_matches = list(re.finditer(r"[\w\u0600-\u06FF]+", text, flags=re.UNICODE))
    if len(token_matches) < 30:
        return text, False

    words = [match.group(0) for match in token_matches]
    run_start = 0
    run_length = 1
    for index in range(1, len(words)):
        if words[index] == words[index - 1]:
            run_length += 1
            if run_length >= 10:
                cut_at = token_matches[run_start].start()
                return text[:cut_at].rstrip() + "\n\n[TRANSCRIPTION_STOPPED_MODEL_REPETITION]", True
        else:
            run_start = index
            run_length = 1

    occurrences = {}
    for index in range(max(0, len(words) - 4)):
        gram = tuple(words[index : index + 5])
        seen = occurrences.setdefault(gram, [])
        seen.append(index)
        if len(seen) >= 8:
            cut_at = token_matches[seen[2]].start()
            return text[:cut_at].rstrip() + "\n\n[TRANSCRIPTION_STOPPED_MODEL_REPETITION]", True

    return text, False


def retry_delay_seconds(error_msg):
    match = re.search(r"retryDelay': '(\d+)s'", error_msg)
    if match:
        return int(match.group(1)) + 5

    match = re.search(r"Please retry in ([\d.]+)s", error_msg)
    if match:
        return int(float(match.group(1))) + 5

    return 60


def is_daily_or_hard_quota(error_msg):
    lowered = error_msg.lower()
    if "generaterequestsperday" in error_msg:
        return True
    if "daily" in lowered and "retry" not in lowered:
        return True
    return False


def build_prompt(title, chunk_number, chunk_count, offset_seconds):
    offset = seconds_to_stamp(offset_seconds)
    return f"""
You are transcribing one short audio chunk from a lecture, class recording, or educational video.

Lecture title: {title}
Chunk: {chunk_number} of {chunk_count}
Chunk start time in the original lecture: {offset}

Return only the transcript for this chunk.

Strict rules:
1. Preserve the spoken language. If the speaker uses Arabic, write it in clear formal Arabic.
2. Keep English technical terms in English when they are spoken or commonly used that way.
3. If the speaker switches languages, preserve the switch naturally.
4. Write mathematical symbols and equations in standard English notation.
5. Put an accurate timestamp at the start of each short paragraph.
6. Add the chunk start offset to timestamps so they refer to the original lecture time.
7. Do not summarize.
8. Do not repeat any sentence, phrase, number, or word unless it is clearly spoken again.
9. If audio is unclear or silent, write [unclear] once and continue only when speech resumes.
10. Stop when this chunk ends.
"""


def transcribe_chunk(
    client,
    types_module,
    model,
    request_delay_seconds,
    chunk_path,
    chunk_text_path,
    title,
    chunk_number,
    chunk_count,
    offset_seconds,
):
    if chunk_text_path.exists():
        return chunk_text_path.read_text(encoding="utf-8")

    prompt = build_prompt(title, chunk_number, chunk_count, offset_seconds)
    last_text = ""
    for attempt in range(1, 3):
        audio_file = client.files.upload(file=str(chunk_path))
        response = client.models.generate_content(
            model=model,
            contents=[prompt, audio_file],
            config=types_module.GenerateContentConfig(
                temperature=0.0,
                candidate_count=1,
            ),
        )
        text = (response.text or "").strip()
        last_text = text
        report = repetition_report(text)
        text, trimmed = trim_looping_text(text)
        trimmed_report = repetition_report(text)

        if text and not looks_repetitive(text):
            chunk_text_path.write_text(text + "\n", encoding="utf-8")
            if trimmed:
                print(
                    f"[~] Trimmed model loop: before words={report['words']} "
                    f"max_run={report['max_run']} max_5gram={report['max_5gram']}; "
                    f"after words={trimmed_report['words']}",
                    flush=True,
                )
            if request_delay_seconds > 0:
                time.sleep(request_delay_seconds)
            return text

        print(
            f"[!] Repetitive output attempt {attempt}: "
            f"words={report['words']} max_run={report['max_run']} max_5gram={report['max_5gram']}",
            flush=True,
        )

    bad_path = chunk_text_path.with_suffix(".rejected.txt")
    bad_path.write_text(last_text + "\n", encoding="utf-8")
    raise RuntimeError(f"Chunk still looked repetitive after retry: {chunk_path}")


def final_path(output_dir, item):
    return output_dir / f"{item.number:02d} - {safe_filename(item.title)} [{safe_filename(item.item_id)}].txt"


def write_index(output_dir, items):
    output_dir.mkdir(parents=True, exist_ok=True)
    lines = [
        "# Transcripts",
        "",
        "| # | Status | Title | Transcript | Source |",
        "|---:|---|---|---|---|",
    ]
    for item in items:
        target = final_path(output_dir, item)
        status = "done" if target.exists() else "pending"
        transcript = f"`{target.name}`" if target.exists() else ""
        source = item.source.replace("|", "\\|")
        lines.append(f"| {item.number} | {status} | {item.title} | {transcript} | {source} |")
    (output_dir / "00_index.md").write_text("\n".join(lines) + "\n", encoding="utf-8")


def transcribe_item(client, types_module, model, request_delay_seconds, output_dir, work_dir, chunk_seconds, item, force):
    target = final_path(output_dir, item)
    if target.exists() and not force:
        print(f"[~] Skipping {item.number:02d}: {item.title} (already complete)", flush=True)
        return

    work_key = safe_filename(f"{item.number:02d}_{item.item_id}_{chunk_seconds}s_{CHUNK_CACHE_VERSION}")
    item_chunk_dir = work_dir / "audio_chunks" / work_key
    item_text_dir = work_dir / "chunk_text" / work_key
    item_text_dir.mkdir(parents=True, exist_ok=True)

    chunks = split_audio(item.media_path, item_chunk_dir, chunk_seconds)
    if not chunks:
        raise RuntimeError(f"No chunks were created for: {item.media_path}")

    print(f"[+] Transcribing {item.number:02d}: {item.title} ({len(chunks)} chunks)", flush=True)
    chunk_texts = []
    for chunk_index, chunk_path in enumerate(chunks, start=1):
        offset = (chunk_index - 1) * chunk_seconds
        chunk_text_path = item_text_dir / f"chunk_{chunk_index:03d}.txt"
        print(f"    chunk {chunk_index}/{len(chunks)} @ {seconds_to_stamp(offset)}", flush=True)
        text = transcribe_chunk(
            client,
            types_module,
            model,
            request_delay_seconds,
            chunk_path,
            chunk_text_path,
            item.title,
            chunk_index,
            len(chunks),
            offset,
        )
        chunk_texts.append((chunk_index, offset, text.strip()))

    body = [
        f"# {item.number:02d} - {item.title}",
        "",
        f"Source: {item.source}",
        f"Media: {item.media_path.name}",
        "",
    ]
    for chunk_index, offset, text in chunk_texts:
        body.extend([f"## Chunk {chunk_index:02d} [{seconds_to_stamp(offset)}]", "", text, ""])

    output_dir.mkdir(parents=True, exist_ok=True)
    target.write_text("\n".join(body).strip() + "\n", encoding="utf-8")
    print(f"[OK] Saved transcript: {target}", flush=True)


def main():
    args = parse_args()
    download_dir = resolve_path(args.download_dir)
    output_dir = resolve_path(args.output_dir)
    work_dir = resolve_path(args.work_dir)
    chunk_seconds = args.chunk_minutes * 60

    if not shutil.which("ffmpeg"):
        raise RuntimeError("ffmpeg was not found on PATH.")

    link_files, media_sources = split_inputs(args.inputs)
    if link_files and not args.skip_download and not args.dry_run:
        download_links(link_files, download_dir, args.cookies_from_browser, args.cookies_file)

    items = build_media_items(link_files, media_sources, download_dir)
    if args.dry_run:
        if items:
            print(f"[+] Found {len(items)} media item(s):", flush=True)
            for item in items:
                payload = {
                    "number": item.number,
                    "status": "ready",
                    "title": item.title,
                    "source": item.source,
                    "media_path": str(item.media_path),
                }
                if args.dry_run_json:
                    print("[DRYRUN] " + json.dumps(payload, ensure_ascii=False), flush=True)
                else:
                    print(f"    {item.number:02d}. {item.title}", flush=True)
                    print(f"        URL: {item.source}", flush=True)
                    print(f"        Media: {item.media_path}", flush=True)
        else:
            links = collect_links(link_files)
            print(f"[+] Found {len(links)} URL(s) waiting for download:", flush=True)
            for number, url in enumerate(links, start=1):
                payload = {
                    "number": number,
                    "status": "will_download",
                    "title": "URL item",
                    "source": url,
                    "media_path": "",
                }
                if args.dry_run_json:
                    print("[DRYRUN] " + json.dumps(payload, ensure_ascii=False), flush=True)
                else:
                    print(f"    {number:02d}. URL item", flush=True)
                    print(f"        URL: {url}", flush=True)
                    print("        Media: will download", flush=True)
        return 0

    if not items:
        print("[~] No media files found to transcribe.", flush=True)
        return 0

    api_key = require_api_key()
    genai_module, errors_module, types_module = load_genai_modules()
    model = args.model or os.environ.get("GEMINI_MODEL", DEFAULT_MODEL)
    client = genai_module.Client(api_key=api_key)

    write_index(output_dir, items)
    print(f"[+] Found {len(items)} media item(s). Output: {output_dir}", flush=True)

    for item in items:
        if item.number < args.start_at:
            continue
        if args.end_at and item.number > args.end_at:
            break

        attempts = 0
        while True:
            try:
                transcribe_item(
                    client,
                    types_module,
                    model,
                    args.request_delay_seconds,
                    output_dir,
                    work_dir,
                    chunk_seconds,
                    item,
                    args.force,
                )
                write_index(output_dir, items)
                break
            except errors_module.APIError as exc:
                attempts += 1
                error_msg = str(exc)
                print(f"[X] Gemini API error on item {item.number:02d}, attempt {attempts}: {error_msg}", flush=True)
                if is_daily_or_hard_quota(error_msg):
                    print("[!] Quota or daily limit reached. Stop here and resume later.", flush=True)
                    return 2
                if attempts >= args.max_api_retries:
                    print(f"[!] Too many temporary API errors on item {item.number:02d}; leaving it pending.", flush=True)
                    break
                delay = retry_delay_seconds(error_msg)
                print(f"[~] Cooling down for {delay} seconds, then retrying the same item.", flush=True)
                time.sleep(delay)
            except Exception as exc:
                print(f"[X] Failed item {item.number:02d}: {exc}", flush=True)
                return 1

    write_index(output_dir, items)
    print("[OK] Transcription run complete.", flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
