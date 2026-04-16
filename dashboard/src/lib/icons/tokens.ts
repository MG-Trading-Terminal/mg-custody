/**
 * Token & chain SVG icons — bundled for air-gap compatibility.
 * No CDN dependencies. All icons are inline SVG strings.
 *
 * Usage:
 *   import { tokenIcon, chainIcon } from '$lib/icons/tokens';
 *   {@html tokenIcon('BTC')}
 *   {@html chainIcon('ethereum')}
 */

// ── Token Icons (by symbol) ─────────────────────────────────────

const TOKEN_ICONS: Record<string, string> = {
  BTC: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#F7931A"/><path d="M22.5 14.2c.3-2.1-1.3-3.2-3.4-4l.7-2.8-1.7-.4-.7 2.7c-.4-.1-.9-.2-1.4-.3l.7-2.7-1.7-.4-.7 2.8c-.3-.1-.7-.2-1-.3l-2.4-.6-.5 1.8s1.3.3 1.2.3c.7.2.8.6.8 1l-.8 3.3c0 .1.1.1.1.2h-.1l-1.2 4.7c-.1.2-.3.6-.8.4 0 0-1.2-.3-1.2-.3l-.8 2 2.2.6c.4.1.8.2 1.2.3l-.7 2.8 1.7.4.7-2.8c.5.1.9.2 1.4.3l-.7 2.8 1.7.4.7-2.8c3 .6 5.2.3 6.1-2.4.8-2.1 0-3.3-1.6-4.1 1.1-.3 2-1 2.2-2.5zm-3.9 5.5c-.6 2.3-4.4 1.1-5.7.7l1-4.1c1.3.3 5.3 1 4.7 3.4zm.6-5.6c-.5 2.1-3.7 1-4.8.8l.9-3.7c1.1.3 4.5.8 3.9 2.9z" fill="#fff"/></svg>`,

  ETH: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#627EEA"/><path d="M16.5 4v8.9l7.5 3.3L16.5 4z" fill="#fff" fill-opacity=".6"/><path d="M16.5 4L9 16.2l7.5-3.3V4z" fill="#fff"/><path d="M16.5 21.9v6.1l7.5-10.4-7.5 4.3z" fill="#fff" fill-opacity=".6"/><path d="M16.5 28V21.9L9 17.6l7.5 10.4z" fill="#fff"/><path d="M16.5 20.6l7.5-4.4-7.5-3.3v7.7z" fill="#fff" fill-opacity=".2"/><path d="M9 16.2l7.5 4.4v-7.7L9 16.2z" fill="#fff" fill-opacity=".6"/></svg>`,

  USDT: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#26A17B"/><path d="M17.9 17.9v-.1c-.1 0-1 .1-2.8.1-1.4 0-2.4-.1-2.8-.1v.1c-4.8-.2-8.3-1.1-8.3-2.1s3.5-1.9 8.3-2.1v3.4c.3 0 1.4.1 2.8.1 1.7 0 2.6-.1 2.8-.1v-3.4c4.8.2 8.3 1.1 8.3 2.1s-3.5 1.9-8.3 2.1zm0-4.5V10h7.8V6.2H6.3V10h7.8v3.4c-5.4.3-9.5 1.5-9.5 2.9s4.1 2.7 9.5 2.9v10.6h3.6V19.2c5.4-.3 9.5-1.5 9.5-2.9s-4-2.7-9.4-2.9z" fill="#fff"/></svg>`,

  USDC: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#2775CA"/><path d="M20.6 18.5c0-2.3-1.4-3.1-4.2-3.4-2-.3-2.4-.8-2.4-1.8s.7-1.6 2.2-1.6c1.3 0 2 .5 2.3 1.4.1.2.2.3.4.3h1c.2 0 .4-.2.3-.4-.3-1.4-1.3-2.5-2.9-2.8V8.5c0-.2-.2-.4-.5-.4h-.9c-.2 0-.4.2-.5.4v1.6c-2 .3-3.2 1.6-3.2 3.1 0 2.2 1.3 3 4.1 3.4 1.8.3 2.5.7 2.5 1.9 0 1.1-1 1.9-2.4 1.9-1.8 0-2.5-.8-2.7-1.8 0-.2-.2-.3-.4-.3h-1c-.3 0-.4.1-.4.4.3 1.7 1.3 2.7 3.3 3.1v1.7c0 .2.2.4.5.4h.9c.3 0 .5-.2.5-.4v-1.7c2-.4 3.4-1.7 3.4-3.4z" fill="#fff"/><path d="M12.6 25.2c-5.1-1.8-7.7-7.5-5.9-12.5.9-2.5 2.9-4.5 5.4-5.4.2-.1.4-.3.4-.6v-.9c0-.2-.1-.4-.4-.4-.1 0-.1 0-.2.1C6 7.4 2.9 14 4.9 19.9c1.2 3.4 3.9 6 7.3 7.2.2.1.4 0 .5-.2 0-.1 0-.1 0-.2v-.9c0-.2-.1-.5-.4-.6h.3zm7-.4c-.2-.1-.4 0-.5.2 0 .1 0 .1 0 .2v.9c0 .3.2.5.4.6 5.1-1.8 7.7-7.5 5.9-12.5-.9-2.5-2.9-4.5-5.4-5.4-.2-.1-.4.1-.5.3v.9c0 .3.2.5.4.6 4 1.5 6.2 5.8 4.8 9.8-.9 2.4-2.7 4.2-5.1 5.1v.3z" fill="#fff"/></svg>`,

  SOL: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#000"/><defs><linearGradient id="sol-g" x1="4" y1="28" x2="28" y2="4"><stop stop-color="#9945FF"/><stop offset=".5" stop-color="#14F195"/><stop offset="1" stop-color="#00C2FF"/></linearGradient></defs><path d="M9.2 20.3c.1-.1.3-.2.5-.2h14.8c.3 0 .5.4.3.6l-2.6 2.6c-.1.1-.3.2-.5.2H6.9c-.3 0-.5-.4-.3-.6l2.6-2.6zm0-11.9c.2-.2.3-.2.5-.2h14.8c.3 0 .5.4.3.6l-2.6 2.6c-.1.1-.3.2-.5.2H6.9c-.3 0-.5-.4-.3-.6l2.6-2.6zm15.3 5.9c-.1-.1-.3-.2-.5-.2H9.2c-.3 0-.5.4-.3.6l2.6 2.6c.1.1.3.2.5.2h14.8c.3 0 .5-.4.3-.6l-2.6-2.6z" fill="url(#sol-g)"/></svg>`,

  TRX: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#EF0027"/><path d="M8 8.6l15.5 2.6-8.3 14.6L8 8.6z" fill="#fff"/><path d="M8 8.6l7.2 8.3L8 8.6zm15.5 2.6l-8.3 6L8 8.9l15.5 2.3z" fill="#fff" fill-opacity=".3"/></svg>`,

  BNB: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#F3BA2F"/><path d="M12.1 14.1L16 10.2l3.9 3.9 2.3-2.3L16 5.6l-6.2 6.2 2.3 2.3zm-6.5 1.9l2.3-2.3 2.3 2.3-2.3 2.3-2.3-2.3zm6.5 1.9L16 21.8l3.9-3.9 2.3 2.3L16 26.4l-6.2-6.2 2.3-2.3zm10.3-1.9l2.3-2.3 2.3 2.3-2.3 2.3-2.3-2.3zM18.8 16L16 13.2 13.2 16l2.8 2.8 2.8-2.8z" fill="#fff"/></svg>`,

  AVAX: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#E84142"/><path d="M20.3 21.5h3c.4 0 .6-.1.8-.4.2-.3.2-.6 0-1l-7.2-12.8c-.2-.4-.5-.5-.9-.5s-.7.2-.9.5l-2.1 3.8c-.2.4-.2.7 0 1.1l5.1 9c.2.3.5.5.9.5h1.3v-.2zm-9.8 0h4.2c.4 0 .6-.1.8-.4.2-.3.2-.6 0-1l-2.1-3.7c-.2-.3-.5-.5-.9-.5-.3 0-.6.2-.8.5l-2.1 3.7c-.2.4-.2.7 0 1 .2.3.5.4.9.4z" fill="#fff"/></svg>`,

  MATIC: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#8247E5"/><path d="M21.1 12.8c-.4-.2-.8-.2-1.1 0l-2.6 1.5-1.8 1-2.6 1.5c-.4.2-.8.2-1.1 0l-2-1.2c-.3-.2-.6-.6-.6-1v-2.3c0-.4.2-.8.6-1l2-.1c.4-.2.8-.2 1.1 0l2 1.2c.3.2.6.6.6 1v1.5l1.8-1v-1.5c0-.4-.2-.8-.6-1l-3.7-2.2c-.4-.2-.8-.2-1.1 0l-3.8 2.2c-.4.2-.6.6-.6 1v4.4c0 .4.2.8.6 1l3.8 2.2c.4.2.8.2 1.1 0l2.6-1.5 1.8-1 2.6-1.5c.4-.2.8-.2 1.1 0l2 1.2c.3.2.6.6.6 1v2.3c0 .4-.2.8-.6 1l-2 1.2c-.4.2-.8.2-1.1 0l-2-1.2c-.3-.2-.6-.6-.6-1v-1.5l-1.8 1V21c0 .4.2.8.6 1l3.8 2.2c.4.2.8.2 1.1 0l3.8-2.2c.4-.2.6-.6.6-1v-4.4c0-.4-.2-.8-.6-1l-3.9-2.8z" fill="#fff"/></svg>`,

  DOT: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#E6007A"/><circle cx="16" cy="9" r="3" fill="#fff"/><circle cx="16" cy="23" r="3" fill="#fff"/><circle cx="10" cy="12" r="2" fill="#fff"/><circle cx="22" cy="12" r="2" fill="#fff"/><circle cx="10" cy="20" r="2" fill="#fff"/><circle cx="22" cy="20" r="2" fill="#fff"/><circle cx="16" cy="16" r="2.5" fill="#fff"/></svg>`,

  ATOM: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#2E3148"/><circle cx="16" cy="16" r="3" fill="#fff"/><ellipse cx="16" cy="16" rx="12" ry="5" fill="none" stroke="#6F7390" stroke-width="1.2"/><ellipse cx="16" cy="16" rx="12" ry="5" fill="none" stroke="#6F7390" stroke-width="1.2" transform="rotate(60 16 16)"/><ellipse cx="16" cy="16" rx="12" ry="5" fill="none" stroke="#6F7390" stroke-width="1.2" transform="rotate(-60 16 16)"/></svg>`,

  NEAR: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#000"/><path d="M21.5 7.5l-4.2 6.3c-.3.4.2.9.6.6l4.1-2.8c.2-.1.4 0 .4.2v9.3c0 .3-.3.4-.5.2l-12-16c-.5-.6-1.2-1-2-1h-.5C6 4.3 5 5.3 5 6.6v18.8c0 1.3 1 2.3 2.3 2.3.8 0 1.5-.4 2-1l4.2-6.3c.3-.4-.2-.9-.6-.6l-4.1 2.8c-.2.1-.4 0-.4-.2V13c0-.3.3-.4.5-.2l12 16c.5.6 1.2 1 2 1h.5c1.3 0 2.3-1 2.3-2.3V6.6c0-1.3-1-2.3-2.3-2.3-.8 0-1.5.4-2 1l.1.2z" fill="#fff"/></svg>`,

  APT: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#000"/><path d="M20.3 10.5h-2.6c-.1 0-.2.1-.3.1l-1.5 2.3h5.6l1.5-2.3c.1-.1 0-.2-.1-.2h-2.6v.1zM11.5 15.5l-1.5 2.3c-.1.1 0 .2.1.2h2.6c.1 0 .2-.1.3-.1l1.5-2.3h-3v-.1zm5.3 0h3l1.5-2.3c.1-.1 0-.2-.1-.2h-2.6c-.1 0-.2.1-.3.1l-1.5 2.4zm-3.6 5.1h2.6c.1 0 .2-.1.3-.1l1.5-2.3H12l-1.5 2.3c-.1.1 0 .2.1.2l2.6-.1z" fill="#fff"/></svg>`,

  SUI: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#4DA2FF"/><path d="M21.3 12.3c1.4 1.8 2.2 4 2.2 6.2 0 4.2-2.9 7.7-6.8 8.7l-.7.2c-.6-.2-1.1-.4-1.6-.7 3.3-.8 5.7-3.8 5.7-7.3 0-1.3-.3-2.6-.9-3.7l-3.3 5c-.2.3-.7.3-.9 0l-3.3-5c-.6 1.1-.9 2.4-.9 3.7 0 3.5 2.4 6.5 5.7 7.3-.5.3-1 .5-1.6.7l-.7-.2C10.5 26.2 7.5 22.7 7.5 18.5c0-2.3.8-4.4 2.2-6.2L15.5 5c.3-.4.7-.4 1 0l4.8 7.3z" fill="#fff"/></svg>`,

  LINK: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#2A5ADA"/><path d="M16 6l-2 1.2-6 3.5L6 11.8v8.4l2 1.1 6 3.5 2 1.2 2-1.2 6-3.5 2-1.1v-8.4l-2-1.1-6-3.5L16 6zm4 14.7l-4 2.3-4-2.3v-4.7l4-2.3 4 2.3v4.7z" fill="#fff"/></svg>`,

  UNI: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#FF007A"/><path d="M13 10c-.5 0-1 .2-1.5.5C10.8 11 10.5 12 11 13c.2.5.5.8 1 1-.2.5-.3 1-.3 1.5 0 2 1.3 3.5 3 4.2.5.2 1 .3 1.5.3 2.5 0 4.5-2 4.5-4.5S18.5 11 16 11c-.3 0-.7 0-1 .1.2-.3.3-.7.3-1.1 0-.5-.5-1-1-1h-1.3z" fill="#fff"/><circle cx="13.5" cy="13.5" r="1" fill="#FF007A"/></svg>`,

  AAVE: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#B6509E"/><path d="M22.5 22h-2l-1.8-4.8-2 5.5c-.1.2-.3.3-.5.3-.2 0-.4-.1-.5-.3l-2-5.5L12 22H9.5l3-8c.1-.3.3-.4.5-.4s.4.1.5.4l2 5.5 2-5.5c.1-.3.3-.4.5-.4s.4.1.5.4l3 8z" fill="#fff"/></svg>`,

  DAI: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#F5AC37"/><path d="M16 6.5c-5.2 0-9.5 4.3-9.5 9.5s4.3 9.5 9.5 9.5 9.5-4.3 9.5-9.5-4.3-9.5-9.5-9.5zm0 15.5c-1.5 0-3-.6-4-1.5h8c-1 .9-2.5 1.5-4 1.5zm-5.5-3h11v-1h-11v1zm0-2.5h11v-1h-11v1zm1.5-3.5c1-1 2.5-1.5 4-1.5s3 .6 4 1.5h-8z" fill="#fff"/></svg>`,

  WBTC: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#201A2D"/><circle cx="16" cy="16" r="11" fill="none" stroke="#F09242" stroke-width="1.5"/><path d="M19.8 14c.2-1.3-.8-2-2.2-2.5l.4-1.8-1.1-.3-.4 1.7c-.3-.1-.6-.1-.9-.2l.4-1.7-1.1-.3-.5 1.8c-.2-.1-.5-.1-.7-.2l-1.5-.4-.3 1.2s.8.2.8.2c.4.1.5.4.5.6L13 14.3c0 .1.1.1.1.1h-.1l-.8 3c-.1.2-.2.4-.6.3 0 0-.8-.2-.8-.2l-.5 1.3 1.4.4c.3.1.5.1.8.2l-.5 1.8 1.1.3.5-1.8c.3.1.6.2.9.2l-.4 1.8 1.1.3.5-1.8c1.9.4 3.3.2 3.9-1.5.5-1.4 0-2.1-1-2.6.7-.2 1.3-.6 1.4-1.6zm-2.5 3.5c-.4 1.4-2.7.7-3.5.5l.6-2.5c.8.2 3.2.6 2.9 2zm.4-3.5c-.3 1.3-2.3.6-2.9.5l.6-2.3c.7.2 2.7.5 2.3 1.8z" fill="#fff"/></svg>`,

  stETH: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#00A3FF"/><path d="M16 5l-5 8.5 5 3 5-3L16 5zm-5 10.5L16 27l5-11.5-5 3-5-3z" fill="#fff"/></svg>`,
};

// ── Chain Icons (by chain name) ─────────────────────────────────

const CHAIN_ICONS: Record<string, string> = {
  ethereum: TOKEN_ICONS.ETH,
  bitcoin: TOKEN_ICONS.BTC,
  tron: TOKEN_ICONS.TRX,
  solana: TOKEN_ICONS.SOL,
  polygon: TOKEN_ICONS.MATIC,
  arbitrum: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#213147"/><path d="M18.8 18.5l1.7 4.5 2.3-1.3-2.3-5.9-1.7 2.7zm-1.6-2.6l-1.7 2.7 3 7.8 2.3-1.3-3.6-9.2zM8.2 22l2.3 1.3 5-12.8H12l-3.8 11.5zm10.5-12.8h-3.5L10 22.7l2.3 1.3 6.4-14.8z" fill="#fff"/></svg>`,
  optimism: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#FF0420"/><path d="M11.5 19.5c-2 0-3.5-1.6-3.5-3.5s1.6-3.5 3.5-3.5c1.4 0 2.6.8 3.1 2h2.3c-.7-2.4-2.9-4.2-5.4-4.2-3.2 0-5.7 2.6-5.7 5.7s2.6 5.7 5.7 5.7c2.5 0 4.7-1.7 5.4-4.2h-2.3c-.5 1.2-1.7 2-3.1 2zm8-7h3.3c1.5 0 2.7.5 2.7 2.2 0 1.3-.8 2.1-2 2.3l2.2 3h-2.4l-1.9-2.8h-.7v2.8H19.5v-7.5zm1.2 3.6h1.6c.8 0 1.5-.2 1.5-1.1 0-.8-.5-1.1-1.4-1.1h-1.7v2.2z" fill="#fff"/></svg>`,
  base: `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#0052FF"/><path d="M16 26c-5.5 0-10-4.5-10-10S10.5 6 16 6c4.8 0 8.8 3.4 9.8 7.9h-6.5c-.8-1.6-2.5-2.7-4.4-2.7-2.8 0-5 2.2-5 5s2.2 5 5 5c1.9 0 3.6-1.1 4.4-2.7h6.5c-1 4.5-5 7.9-9.8 7.5z" fill="#fff"/></svg>`,
  bsc: TOKEN_ICONS.BNB,
  avalanche: TOKEN_ICONS.AVAX,
  cosmos: TOKEN_ICONS.ATOM,
  polkadot: TOKEN_ICONS.DOT,
  near: TOKEN_ICONS.NEAR,
  aptos: TOKEN_ICONS.APT,
  sui: TOKEN_ICONS.SUI,
};

// ── Fallback ────────────────────────────────────────────────────

const FALLBACK_ICON = `<svg viewBox="0 0 32 32" xmlns="http://www.w3.org/2000/svg"><circle cx="16" cy="16" r="16" fill="#555"/><text x="16" y="21" text-anchor="middle" fill="#fff" font-size="14" font-family="monospace">?</text></svg>`;

// ── Public API ──────────────────────────────────────────────────

/** Get SVG icon for a token symbol (e.g. "BTC", "USDT") */
export function tokenIcon(symbol: string, size = 24): string {
  const svg = TOKEN_ICONS[symbol.toUpperCase()] || FALLBACK_ICON;
  return svg.replace('viewBox=', `width="${size}" height="${size}" viewBox=`);
}

/** Get SVG icon for a chain (e.g. "ethereum", "bitcoin") */
export function chainIcon(chain: string, size = 20): string {
  const svg = CHAIN_ICONS[chain.toLowerCase()] || FALLBACK_ICON;
  return svg.replace('viewBox=', `width="${size}" height="${size}" viewBox=`);
}

/** Check if a token icon exists */
export function hasTokenIcon(symbol: string): boolean {
  return symbol.toUpperCase() in TOKEN_ICONS;
}

/** List all available token symbols with icons */
export function availableTokenIcons(): string[] {
  return Object.keys(TOKEN_ICONS);
}

/** List all available chain names with icons */
export function availableChainIcons(): string[] {
  return Object.keys(CHAIN_ICONS);
}
