export const APP_VERSION = __APP_VERSION__;

export function formatVersion(version: string | null | undefined) {
  return version ? `v${version}` : "Unavailable";
}
