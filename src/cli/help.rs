pub(super) fn print() {
    println!(
        r#"Syousetu Translate (Rust)
  inspect --url URL [--html saved.html]
  inspect-dat --dir PATH [--out inspection.json]
  inspect-root-dll --dll PATH [--out root-report.json]
  inspect-ehnd --dir PATH [--out ehnd-report.json]
  audit-ehnd-regex --dir PATH [--out audit.json] [--strict]
  audit-ehnd-control --dir PATH [--out control-audit.json]
  Experimental: --ehnd-dir PATH --ehnd-regex-experimental [--ehnd-regex-strict]
  compare-engines --eztrans-dir PATH --input corpus.txt [--out comparison.json]
  Experimental: --ehnd-dir PATH --ehnd-literal-only (native only)
  download --url URL --out novel.txt [--start N --end N]
  file --input novel.txt --out translated.txt
  inspect-config [--config config/config.properties]
  write-config --out config/config.properties [--json edited-settings.json]

  Engine: --engine internal|passthrough (default: internal, SMaLL-100 INT8)
  Compatibility commands/engines listed above require --features legacy at build time.
  models [--prepare | --verify] [--model-dir PATH]
  translate --text JAPANESE [--model-dir PATH]
  memory [--learn | --forget] --source JAPANESE [--corrected KOREAN] [--previous OLD_KOREAN]
  memory --import-ehnd EXTRACTED_DIRECTORY [--memory-path FILE]
  memory --import-dictionary USERDICT.jk_OR_TSV [--memory-path FILE]
  Internal: --model-dir PATH, --memory-path PATH, --no-learning
  Native: --native-dictionary PATH [--native-strict]
  Legacy: --engine eztrans [--eztrans-dir PATH]
  Common: --config PATH, --dictionary PATH, --japanese, --no-japanese, --jobs 1..16
  File: --input-encoding shift_jis|euc-kr|utf-8|utf-16le|latin1
  Download: --furigana --include-comments --comment-separator TEXT
  Output: --title-format TEXT --page-number-format 000 --output-dir DIR --auto-output
  Network: --delay-ms 1200 --retry 2 --user-agent TEXT [--ca-bundle certificates.pem]
  Offline index pagination: --html first.html --index-html-dir DIR (2.html, 3.html, ...)
  Progress events: --progress-json (JSON lines on stderr)

  Native mode is limited glossary translation and is not identical to ezTrans.
  --passthrough is only for parser tests, not Japanese-to-Korean translation.
  Do not redistribute proprietary vendor assets without permission."#
    );
}
