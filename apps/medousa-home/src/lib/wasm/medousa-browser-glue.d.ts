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
  export function pair_from_invite(qrUrl: string, displayName: string): Promise<string>;
  export function set_active_portal(workshopId: string): void;
  export function forget_portal(workshopId: string): void;
  export function portal_request(method: string, path: string, body: string): Promise<string>;
  export function portal_open_stream(
    kind: string,
    path: string,
    accept: string,
    onEvent: (data: string) => void,
    onError: (message: string) => void,
  ): void;
  export function portal_stop_streams(prefix: string): void;
  export function read_vault(path: string): Promise<string>;
  export function write_vault(path: string, body: string): Promise<void>;
  export default function init(input?: unknown): Promise<unknown>;
}
