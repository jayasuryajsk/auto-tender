# Markdown

Markdown support is available natively in Zed.

- Tree-sitter: [tree-sitter-markdown](https://github.com/tree-sitter-grammars/tree-sitter-markdown)
- Language Server: N/A

## Document File Support

Zed supports automatic conversion of various document formats to Markdown for both semantic indexing and assistant tools, using [Microsoft MarkItDown](https://github.com/microsoft/markitdown):

**Supported Document Formats:**
- **PDF files** (`.pdf`)
- **Microsoft Office** (`.docx`, `.pptx`, `.xlsx`, `.xls`, `.xlsm`, `.xlsb`, `.xla`, `.xlam`)
- **OpenDocument** (`.odt`, `.ods`, `.odp`)
- **Images** (`.jpg`, `.jpeg`, `.png`, `.gif`, `.bmp`, `.tiff`, `.webp`, `.ico`, `.svg`) - *with OCR support*
- **Audio files** (`.wav`, `.mp3`, `.m4a`, `.aac`, `.ogg`, `.flac`) - *with speech transcription*
- **Web formats** (`.html`, `.htm`, `.xml`, `.json`)
- **Text formats** (`.csv`, `.tsv`, `.txt`)
- **E-books** (`.epub`)
- **Archives** (`.zip`)
- **Email** (`.msg`, `.eml`)

**Advanced Features:**
- **OCR (Optical Character Recognition)**: Images are automatically processed to extract text content
- **Speech Transcription**: Audio files are transcribed to text using AI
- **Structured Data**: JSON, XML, and CSV files are converted to readable Markdown tables
- **Web Content**: HTML pages are converted while preserving structure

When these documents are indexed for semantic search or accessed via assistant tools, they are automatically converted to Markdown format for processing. This requires `markitdown` to be installed:

```bash
pip install markitdown[all]
```

## Syntax Highlighting Code Blocks

Zed supports language-specific syntax highlighting of markdown code blocks by leveraging [tree-sitter language grammars](../extensions/languages.md#grammar). All [Zed supported languages](../languages.md), including those provided by official or community extensions, are available for use in markdown code blocks. All you need to do is provide a language name after the opening <kbd>```</kbd> code fence like so:

````python
```python
import functools as ft

@ft.lru_cache(maxsize=500)
def fib(n):
    return n if n < 2 else fib(n - 1) + fib(n - 2)
```
````

## Configuration

### Format

Zed supports using Prettier to automatically re-format Markdown documents. You can trigger this manually via the {#action editor::Format} action or via the {#kb editor::Format} keyboard shortcut. Alternately, you can automatically format by enabling [`format_on_save`](./configuring-zed.md#format-on-save) in your settings.json:

```json
  "languages": {
    "Markdown": {
      "format_on_save": "on"
    }
  },
```

### Trailing Whitespace

By default Zed will remove trailing whitespace on save. If you rely on invisible trailing whitespace being converted to `<br />` in Markdown files you can disable this behavior with:

```json
  "languages": {
    "Markdown": {
      "remove_trailing_whitespace_on_save": false
    }
  },
```
