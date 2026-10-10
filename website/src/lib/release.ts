// Resolves the installers of the latest GitHub release at build time.
// If the API is unreachable (offline build, rate limit), every link falls back
// to the "latest release" page, so the site always builds.

export const REPO = 'LeoMartinDev/Plume';
export const REPO_URL = `https://github.com/${REPO}`;
export const LATEST_URL = `${REPO_URL}/releases/latest`;

export type Platform = 'mac' | 'windows' | 'linux';

export interface ReleaseInfo {
  version: string | null;
  assets: Record<Platform, string>;
}

const MATCHERS: Record<Platform, (name: string) => boolean> = {
  mac: (n) => n.endsWith('.dmg'),
  windows: (n) => n.endsWith('.exe'),
  linux: (n) => n.endsWith('.deb'),
};

let cached: Promise<ReleaseInfo> | undefined;

export function getLatestRelease(): Promise<ReleaseInfo> {
  cached ??= load();
  return cached;
}

async function load(): Promise<ReleaseInfo> {
  const fallback: ReleaseInfo = {
    version: null,
    assets: { mac: LATEST_URL, windows: LATEST_URL, linux: LATEST_URL },
  };
  try {
    const headers: Record<string, string> = { Accept: 'application/vnd.github+json' };
    const token = process.env.GITHUB_TOKEN;
    if (token) headers.Authorization = `Bearer ${token}`;
    const res = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
      headers,
      signal: AbortSignal.timeout(8000),
    });
    if (!res.ok) throw new Error(`GitHub API ${res.status}`);
    const data = (await res.json()) as {
      tag_name?: string;
      assets?: { name: string; browser_download_url: string }[];
    };
    const assets = { ...fallback.assets };
    for (const platform of Object.keys(MATCHERS) as Platform[]) {
      const asset = data.assets?.find((a) => MATCHERS[platform](a.name));
      if (asset) assets[platform] = asset.browser_download_url;
    }
    return { version: data.tag_name ?? null, assets };
  } catch (error) {
    console.warn(`[release] Using fallback links: ${(error as Error).message}`);
    return fallback;
  }
}
