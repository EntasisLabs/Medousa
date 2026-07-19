declare module "html2canvas" {
  interface Html2CanvasOptions {
    scale?: number;
    useCORS?: boolean;
    backgroundColor?: string | null;
    scrollX?: number;
    scrollY?: number;
    windowWidth?: number;
    width?: number;
    height?: number;
    logging?: boolean;
    onclone?: (doc: Document) => void;
  }

  function html2canvas(
    element: HTMLElement,
    options?: Html2CanvasOptions,
  ): Promise<HTMLCanvasElement>;

  export default html2canvas;
}
