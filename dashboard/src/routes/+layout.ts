// Static adapter: disable SSR, enable client-side rendering only
export const prerender = false;
export const ssr = false;

import { redirect } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';

const PUBLIC_PATHS = ['/login', '/register', '/forgot'];
const AUTH_FLAG_KEY = 'custody_authed';

export const load: LayoutLoad = ({ url }) => {
	if (PUBLIC_PATHS.some(p => url.pathname.startsWith(p))) {
		return {};
	}

	// The real credential is the HttpOnly session cookie, which this
	// script cannot read. We rely on a lightweight flag to steer
	// client-side routing; an invalid/absent cookie will bounce back
	// via the 401 handler in `api.ts` on the next request.
	if (typeof window !== 'undefined') {
		const authed = localStorage.getItem(AUTH_FLAG_KEY) === '1';
		// Back-compat: older builds stored a token key directly.
		const legacy = localStorage.getItem('custody_token');
		if (!authed && !legacy) {
			redirect(302, '/login');
		}
	}

	return {};
};
