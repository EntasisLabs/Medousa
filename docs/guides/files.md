# Open files in Medousa

Open **Files** and select a file from an added folder. Supported files open in
Medousa tabs without importing them into Notes or creating a Code project.
Folders and files remain on the workshop machine. Local folder selection and
file opening are available when the desktop app and workshop share that disk.

| Type | In Medousa |
|------|------------|
| Markdown | Rendered preview, editable source, or split view |
| Text, source, and configuration files | The same source editor and syntax highlighting used in Code |
| PDF | Paged document preview with fit-width and zoom controls |
| Images | Image preview, including files outside the vault |
| CSV, TSV, XLSX | Existing read-only spreadsheet preview |

Markdown links and images resolve relative to the original file. External web
links open in the Browser. HTML files open as source rather than running as a
web page. Image decoding depends on the platform; an unreadable image offers an
external-app fallback. Other unsupported formats use **Open in app**.

For text and Markdown, **Save** or **⌘S / Ctrl+S** writes to the original file.
Unsaved drafts remain available while switching tabs. Closing an unsaved file
asks before discarding the draft. If the file changed outside Medousa, Save
keeps your draft and reports the conflict. **Reload** reads the file again and
asks before replacing an unsaved draft.

Text files currently load up to 2,000,000 bytes. Image, PDF, and spreadsheet
payloads load up to 12,000,000 bytes. The spreadsheet preview displays the first
sheet, up to 500 data rows and 32 columns; it is not a spreadsheet editor or
formula engine. PDF previews render one page at a time and release their worker
when closed. Closing an image preview releases its temporary file URLs.
