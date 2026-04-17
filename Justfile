# BitDB project tasks
# Run any recipe with:  just <recipe>

slides_md  := "docs/slides.md"
slides_pdf := "docs/slides.pdf"
cover_md   := "docs/cover.md"
report_md  := "docs/report.md"
report_pdf := "docs/report.pdf"
cover_pdf  := "docs/cover_tmp.pdf"

# ── slides ────────────────────────────────────────────────────────────────────

# Build the PDF presentation (requires pandoc + LaTeX in PATH / nix shell)
slides:
    pandoc {{slides_md}} \
        -t beamer \
        --pdf-engine=xelatex \
        --resource-path=docs \
        -o {{slides_pdf}}
    @echo "Built {{slides_pdf}}"

# Open the PDF after building (Linux: xdg-open; macOS: open)
slides-open: slides
    xdg-open {{slides_pdf}} 2>/dev/null || open {{slides_pdf}}

# Build the project report PDF (cover + body merged)
report:
    #!/usr/bin/env bash
    set -euo pipefail
    ABS=$(pwd)/docs/attachments
    TMP=$(mktemp /tmp/report_XXXXXX.md)
    sed "s|{tracer_result.png}|{$ABS/tracer_result.png}|g; \
         s|{merge_bench.png}|{$ABS/merge_bench.png}|g; \
         s|{tracer_1.png}|{$ABS/tracer_1.png}|g; \
         s|{tracer_2.png}|{$ABS/tracer_2.png}|g" \
        docs/report.md > "$TMP"
    COVER_TMP=$(mktemp /tmp/cover_XXXXXX.md)
    sed "s|CIT_LOGO|$ABS/cit.png|g" docs/cover.md > "$COVER_TMP"
    pandoc "$COVER_TMP" \
        --pdf-engine=xelatex \
        -V geometry:"top=1in,bottom=1in,left=1.25in,right=1.25in" \
        -V papersize=a4 -V fontsize=12pt --standalone \
        -o docs/cover_tmp.pdf
    rm -f "$COVER_TMP"
    pandoc "$TMP" \
        --pdf-engine=xelatex \
        -V papersize=a4 -V fontsize=12pt --standalone \
        -o docs/report_body.pdf
    gs -dBATCH -dNOPAUSE -q -sDEVICE=pdfwrite \
        -dPDFSETTINGS=/prepress \
        -sOutputFile=docs/report.pdf \
        docs/cover_tmp.pdf docs/report_body.pdf
    rm -f docs/cover_tmp.pdf docs/report_body.pdf "$TMP"
    echo "Built docs/report.pdf"

# Open the report PDF after building
report-open: report
    xdg-open {{report_pdf}} 2>/dev/null || open {{report_pdf}}

# ── rust ──────────────────────────────────────────────────────────────────────

build:
    cargo build --all

test:
    cargo test --all-targets --all-features

bench:
    cargo bench --bench engine -- --sample-size 10

fmt:
    cargo fmt --all

lint:
    cargo clippy --all-targets --all-features -- -D warnings

# Run the tracer TUI (generates data in ./tracer-data by default)
tracer:
    cargo run -p tracer -- --data-dir ./tracer-data

# ── combined ──────────────────────────────────────────────────────────────────

# Full CI pass: fmt check + lint + test + slides
ci: fmt lint test slides
