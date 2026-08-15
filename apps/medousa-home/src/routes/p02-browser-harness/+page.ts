import { dev } from "$app/environment";
import { env } from "$env/dynamic/public";
import { p02HarnessAvailable } from "$lib/bench/p02Availability";
import { error } from "@sveltejs/kit";

export const ssr = false;

export function load() {
  if (!p02HarnessAvailable(dev, env.PUBLIC_P02_HARNESS)) error(404, "Not found");
}
