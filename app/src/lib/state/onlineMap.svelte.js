// Whether the online map and place search may use the network (D17). The backend owns the value and its default per
// build (`Catalog::online_map_enabled`). This store mirrors it so the map, the metadata panel and Settings change
// together when the person flips it in any of them.

import { getMapSettings, setOnlineMapEnabled } from "$lib/api/map.js";

export class OnlineMapStore {
  /** `null` until the backend has answered. */
  enabled = $state(/** @type {boolean | null} */ (null));

  /** Reads every map setting and keeps the switch. @returns {Promise<import('$lib/api/map.js').MapSettings>} */
  async refresh() {
    const settings = await getMapSettings();
    this.enabled = settings.online_map_enabled;
    return settings;
  }

  /** Changes only after the backend has stored the choice, so the view never shows a state that was not saved. */
  async set(/** @type {boolean} */ enabled) {
    await setOnlineMapEnabled(enabled);
    this.enabled = enabled;
  }
}

export const onlineMap = new OnlineMapStore();
