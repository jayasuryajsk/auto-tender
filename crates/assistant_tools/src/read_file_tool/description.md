Reads the content of the given file in the project.

- For text-based files, the content is read directly.
- For document formats such as PDF (.pdf), Microsoft Word (.docx), OpenDocument Text (.odt), and PowerPoint (.pptx), the tool will attempt to convert the document to Markdown and return the Markdown content.
- If line numbers (`start_line`, `end_line`) are provided, they apply to the (potentially converted) text content.
- If the file is very large and no line numbers are specified, an outline or summary may be returned instead of the full content.
- Never attempt to read a path that hasn't been previously mentioned by the user or other tools.
