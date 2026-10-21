import type { ReviewPerson } from "$lib/types/generated/daemon_api";
/** Providers send chronological normalized decisions. A later comment isn't a new decision. */
export function reviewPeople(history: ReviewPerson[], requested: string[]) {
  const people = new Map<string, ReviewPerson>();
  for (const person of history) {
    const previous = people.get(person.id);
    if (person.state === "commented" && previous?.state && previous.state !== "commented") continue;
    people.set(person.id, person);
  }
  for (const id of requested) people.set(id, { id, name: people.get(id)?.name ?? id, state: "pending" });
  return [...people.values()];
}
