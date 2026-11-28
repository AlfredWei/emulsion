// Which map tiles the Library map draws, and the credit that goes with them (RFC-0038 B5.12). The defaults are
// OpenStreetMap's public tile server; Settings > Map can point the map at another provider. The backend
// (`map_endpoints.rs`) has already validated any saved value, and a value it rejected arrives here as `null`.

export const DEFAULT_TILE_URL = "https://tile.openstreetmap.org/{z}/{x}/{y}.png";
export const OSM_COPYRIGHT_URL = "https://www.openstreetmap.org/copyright";

/** @param {string} text */
function escapeHtml(text) {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#39;");
}

/**
 * The tile layer's address and credit line. A custom provider's credit is the person's own plain text, escaped, so
 * it can never inject markup; the default keeps OpenStreetMap's required link.
 * @param {{ tile_url?: string | null, attribution?: string | null } | null | undefined} settings
 * @returns {{ url: string, attributionHtml: string, custom: boolean }}
 */
export function tileLayerConfig(settings) {
  const url = settings?.tile_url?.trim();
  if (url) {
    const credit = settings?.attribution?.trim();
    return { url, attributionHtml: escapeHtml(credit || "Custom map tiles"), custom: true };
  }
  return {
    url: DEFAULT_TILE_URL,
    attributionHtml: `© <a href="${OSM_COPYRIGHT_URL}">OpenStreetMap</a> contributors`,
    custom: false,
  };
}

/**
 * The server the map downloads tiles from, named on the card that turns the online map on. Read with a pattern, not
 * `URL`, because a template's `{s}` style placeholders are not valid in a host name.
 * @param {Parameters<typeof tileLayerConfig>[0]} settings
 */
export function tileServerHost(settings) {
  const url = tileLayerConfig(settings).url;
  return /^https?:\/\/([^/?#]+)/i.exec(url)?.[1] ?? url;
}
