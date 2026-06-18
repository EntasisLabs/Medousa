export interface UserProfileRecord {
  profile_id: string;
  display_name: string;
  created_at: string;
  is_default: boolean;
  archived?: boolean;
}

export interface ListUserProfilesResponse {
  profiles: UserProfileRecord[];
  active_profile_id: string;
  resolved_user_id: string;
}

export interface CreateUserProfileResponse {
  profile: UserProfileRecord;
  active_profile_id: string;
  resolved_user_id: string;
}

export interface SetActiveUserProfileResponse {
  active_profile_id: string;
  resolved_user_id: string;
}
