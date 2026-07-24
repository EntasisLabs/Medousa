/** Cross-component You / profiles chrome actions (rail toolbar → panel). */

export const PROFILES_ADD_PERSON_EVENT = "medousa-profiles-add-person";
export const PROFILES_FOCUS_TEACH_EVENT = "medousa-profiles-focus-teach";
export const PROFILES_ADD_PROFILE_EVENT = "medousa-profiles-add-profile";

export function dispatchProfilesAddPerson() {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(PROFILES_ADD_PERSON_EVENT));
}

export function dispatchProfilesFocusTeach() {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(PROFILES_FOCUS_TEACH_EVENT));
}

export function dispatchProfilesAddProfile() {
  if (typeof window === "undefined") return;
  window.dispatchEvent(new CustomEvent(PROFILES_ADD_PROFILE_EVENT));
}
