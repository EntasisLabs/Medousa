export function p02HarnessAvailable(development: boolean, buildFlag: string | undefined): boolean {
  return development || buildFlag === "1";
}
