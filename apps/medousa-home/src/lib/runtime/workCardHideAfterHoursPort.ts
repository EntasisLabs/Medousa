/** Port so workspace does not import the settings store. */

let hideAfterHours: () => number = () => 0;

export function setWorkCardHideAfterHoursPort(port: (() => number) | null): void {
  hideAfterHours = port ?? (() => 0);
}

export function workCardHideAfterHours(): number {
  return hideAfterHours();
}
