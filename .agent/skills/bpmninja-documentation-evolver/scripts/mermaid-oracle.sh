#!/usr/bin/env bash
# Mermaid Rendering Oracle

set -e

echo "=== Mermaid Diagram Oracle ==="

# Prüft alle Mermaid-Dateien auf Syntax-Fehler
find docs -name "*.mmd" -o -name "*.mermaid" | while read file; do
  mmdc -i "$file" -o /tmp/test.png || { echo "FAIL: Mermaid rendering failed for $file"; exit 1; }
done

echo "PASS: All Mermaid diagrams render correctly"
