// Static adapter: disable SSR, enable client-side rendering only
export const prerender = false;
export const ssr = false;

import { redirect } from '@sveltejs/kit';
import type { LayoutLoad } from './$types';

const PUBLIC_PATHS = ['/login'];

export const load: LayoutLoad = ({ url }) => {
	// Skip auth check for public pages
	if (PUBLIC_PATHS.some(p => url.pathname.startsWith(p))) {
		return {};
	}

	// Client-side only — check localStorage for token
	if (typeof window !== 'undefined') {
		const token = localStorage.getItem('custody_token');
		if (!token) {
			redirect(302, '/login');
		}
	}

	return {};
};
