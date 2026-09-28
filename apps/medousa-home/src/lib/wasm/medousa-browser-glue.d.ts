/** Glue emitted by `scripts/build-wasm-daemon.sh` into `static/wasm`. */
declare module "/wasm/medousa_browser.js" {
  export function boot(): Promise<void>;
  export function configure_inference(baseUrl: string, apiKey: string, model: string): void;
  export function list_sessions(): Promise<string>;
  export function create_session(title: string): Promise<string>;
  export function list_notes(): Promise<string>;
  export function save_note(noteId: string, title: string, body: string): Promise<string>;
  export function start_turn(
    sessionId: string,
    text: string,
    onEvent: (event: string) => void,
  ): Promise<string>;
  export function run_grapheme(source: string): string;
  export function dial_iroh_ticket(ticket: string, path: string): Promise<string>;
  export function read_vault(path: string): Promise<string>;
  export function write_vault(path: string, body: string): Promise<void>;
  export default function init(input?: unknown): Promise<unknown>;
}
