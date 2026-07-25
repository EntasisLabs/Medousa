/** Shared You / profiles focus selection (rail list ↔ blob field). */
class ProfilesSelectionStore {
  selectedId = $state<string | null>(null);

  select(id: string | null) {
    this.selectedId = id;
  }
}

export const profilesSelection = new ProfilesSelectionStore();
