import {
  createBot,
  duplicateBot,
  listBots,
  openBot,
  setBotArchived,
  updateBot,
} from "$lib/daemon/bot";
import type {
  BotListResponse,
  BotOpenResponse,
  BotProfile,
  CreateBotRequest,
  DuplicateBotRequest,
  SetBotArchivedRequest,
  UpdateBotRequest,
} from "$lib/types/generated/daemon_api";
import { activeWorkshopId } from "$lib/utils/workshopLocality";

export interface BotStoreApi {
  list(): Promise<BotListResponse>;
  create(request: CreateBotRequest): Promise<BotOpenResponse>;
  update(botId: string, request: UpdateBotRequest): Promise<BotProfile>;
  setArchived(botId: string, request: SetBotArchivedRequest): Promise<BotProfile>;
  duplicate(botId: string, request?: DuplicateBotRequest): Promise<BotOpenResponse>;
  open(botId: string): Promise<BotOpenResponse>;
}

const defaultApi: BotStoreApi = {
  list: listBots,
  create: createBot,
  update: updateBot,
  setArchived: setBotArchived,
  duplicate: duplicateBot,
  open: openBot,
};

export class BotStore {
  bots = $state<BotProfile[]>([]);
  loading = $state(false);
  error = $state<string | null>(null);
  workshopScopeId = activeWorkshopId();
  private epoch = 0;
  private loaded = false;
  private refreshInFlight: Promise<void> | null = null;

  constructor(private readonly api: BotStoreApi = defaultApi) {}

  resetForWorkshopSwitch() {
    this.epoch += 1;
    this.workshopScopeId = "";
    this.bots = [];
    this.error = null;
    this.loading = false;
    this.loaded = false;
    this.refreshInFlight = null;
  }

  activateWorkshopScope(workshopId: string) {
    const scope = workshopId.trim();
    if (!scope || scope === this.workshopScopeId) return;
    this.resetForWorkshopSwitch();
    this.workshopScopeId = scope;
  }

  async refresh(options: { force?: boolean } = {}): Promise<void> {
    if (this.loaded && !options.force) return;
    if (this.refreshInFlight) return this.refreshInFlight;
    const epoch = this.epoch;
    this.loading = true;
    this.error = null;
    const request = this.api
      .list()
      .then((response) => {
        if (epoch !== this.epoch) return;
        this.bots = response.bots;
        this.loaded = true;
      })
      .catch((error) => {
        if (epoch !== this.epoch) return;
        this.error = error instanceof Error ? error.message : String(error);
        throw error;
      })
      .finally(() => {
        if (epoch === this.epoch) {
          this.loading = false;
          this.refreshInFlight = null;
        }
      });
    this.refreshInFlight = request;
    return request;
  }

  forSession(sessionId: string): BotProfile | null {
    const id = sessionId.trim();
    if (!id) return null;
    return this.bots.find((bot) => bot.primary_session_id === id) ?? null;
  }

  async create(request: CreateBotRequest): Promise<BotOpenResponse> {
    const response = await this.api.create(request);
    this.upsert(response.bot);
    return response;
  }

  async update(bot: BotProfile, request: Omit<UpdateBotRequest, "expected_revision">) {
    const updated = await this.api.update(bot.bot_id, {
      ...request,
      expected_revision: bot.revision,
    });
    this.upsert(updated);
    return updated;
  }

  async setArchived(bot: BotProfile, archived: boolean): Promise<BotProfile> {
    const updated = await this.api.setArchived(bot.bot_id, {
      archived,
      expected_revision: bot.revision,
    });
    this.upsert(updated);
    return updated;
  }

  async duplicate(
    bot: BotProfile,
    request: DuplicateBotRequest = {},
  ): Promise<BotOpenResponse> {
    const response = await this.api.duplicate(bot.bot_id, request);
    this.upsert(response.bot);
    return response;
  }

  async open(bot: BotProfile): Promise<BotOpenResponse> {
    const response = await this.api.open(bot.bot_id);
    this.upsert(response.bot);
    return response;
  }

  private upsert(bot: BotProfile) {
    this.loaded = true;
    this.error = null;
    this.bots = [
      bot,
      ...this.bots.filter((candidate) => candidate.bot_id !== bot.bot_id),
    ].sort((left, right) => {
      if (left.archived !== right.archived) return left.archived ? 1 : -1;
      return Date.parse(right.updated_at) - Date.parse(left.updated_at);
    });
  }
}

export const bots = new BotStore();
