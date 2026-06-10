declare module "html2pdf.js" {
  interface Html2PdfWorker {
    set(options: Record<string, unknown>): Html2PdfWorker;
    from(element: HTMLElement): Html2PdfWorker;
    save(): Promise<void>;
    outputPdf(type: "blob"): Promise<Blob>;
  }

  function html2pdf(): Html2PdfWorker;
  export default html2pdf;
}
