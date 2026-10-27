#!/usr/bin/env python3
"""Chạy scripts/metrics.sql trên cơ sở dữ liệu của hub và in từng truy vấn thành bảng (docs/PLAN.md T20 · ARC-41).

Dành cho máy không có `sqlite3` CLI (Windows). Chỉ dùng thư viện chuẩn; mở cơ sở dữ liệu ở chế độ CHỈ ĐỌC.

    python scripts/metrics.py                      # đường dẫn mặc định của hub
    python scripts/metrics.py path/to/arcturus.db
"""
import os
import sqlite3
import sys
from pathlib import Path

SQL = Path(__file__).with_name("metrics.sql")


def default_db() -> Path:
    if os.name == "nt":
        base = Path(os.environ.get("APPDATA", Path.home() / "AppData" / "Roaming"))
    elif sys.platform == "darwin":
        base = Path.home() / "Library" / "Application Support"
    else:
        base = Path(os.environ.get("XDG_DATA_HOME", Path.home() / ".local" / "share"))
    return base / "arcturus" / "arcturus.db"


def queries(text: str):
    """Tách theo dòng `-- @query <tên>`; trả về (tên, câu lệnh)."""
    out, name, buf = [], None, []
    for line in text.splitlines():
        if line.startswith("-- @query "):
            if name:
                out.append((name, "\n".join(buf)))
            name, buf = line[len("-- @query "):].strip(), []
        elif name:
            buf.append(line)
    if name:
        out.append((name, "\n".join(buf)))
    return out


def table(cols, rows) -> str:
    cells = [[("" if v is None else str(v)) for v in r] for r in rows]
    widths = [max([len(c)] + [len(r[i]) for r in cells]) for i, c in enumerate(cols)]
    line = lambda r: "  ".join(v.ljust(w) for v, w in zip(r, widths)).rstrip()
    return "\n".join([line(cols), line(["-" * w for w in widths])] + [line(r) for r in cells])


def main() -> int:
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")  # Windows mặc định cp1252, không in được tiếng Việt
    db = Path(sys.argv[1]) if len(sys.argv) > 1 else default_db()
    if not db.is_file():
        print(f"metrics.py: không thấy cơ sở dữ liệu {db}", file=sys.stderr)
        return 2
    conn = sqlite3.connect(f"{db.resolve().as_uri()}?mode=ro", uri=True)
    for name, sql in queries(SQL.read_text(encoding="utf-8")):
        cur = conn.execute(sql)
        cols = [d[0] for d in cur.description]
        rows = cur.fetchall()
        print(f"== {name} ({len(rows)} dòng) ==")
        print(table(cols, rows) if rows else "(không có dữ liệu)")
        print()
    return 0


if __name__ == "__main__":
    sys.exit(main())
