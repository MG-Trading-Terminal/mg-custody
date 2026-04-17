/**
 * API client for MG Custody backend.
 *
 * The session token is carried in an HttpOnly cookie (`custody_session`)
 * that the backend installs on login / verify-2fa. The browser attaches
 * it automatically as long as `credentials: 'include'` is set, so we do
 * NOT keep the raw token in JavaScript memory or localStorage — an XSS
 * on the dashboard can't exfiltrate an HttpOnly cookie.
 *
 * We still keep a lightweight `authed` flag in localStorage so pages can
 * tell, without a round-trip, whether the user is logged in. The flag is
 * not a credential: losing it just triggers a harmless redirect.
 */

const BASE_URL = '/';
const AUTH_FLAG_KEY = 'custody_authed';

function headers(): HeadersInit {
	return {
		'Content-Type': 'application/json',
	};
}

async function request(method: string, path: string, body?: unknown): Promise<Response> {
	const url = `${BASE_URL}${path.startsWith('/') ? path.slice(1) : path}`;
	const opts: RequestInit = {
		method,
		headers: headers(),
		credentials: 'include',
	};
	if (body !== undefined) {
		opts.body = JSON.stringify(body);
	}
	const resp = await fetch(url, opts);
	if (resp.status === 401) {
		if (typeof window !== 'undefined') {
			localStorage.removeItem(AUTH_FLAG_KEY);
			if (!window.location.pathname.startsWith('/login')) {
				window.location.href = '/login';
			}
		}
	}
	return resp;
}

export const api = {
	async get(path: string): Promise<string> {
		const resp = await request('GET', path);
		return resp.text();
	},

	async getJson<T = unknown>(path: string): Promise<T> {
		const resp = await request('GET', path);
		if (!resp.ok) throw new Error(`API error: ${resp.status}`);
		return resp.json();
	},

	async post<T = unknown>(path: string, body?: unknown): Promise<T> {
		const resp = await request('POST', path, body);
		if (!resp.ok) {
			const err = await resp.json().catch(() => ({ message: resp.statusText }));
			throw new Error(err.message || `API error: ${resp.status}`);
		}
		return resp.json();
	},

	async put<T = unknown>(path: string, body?: unknown): Promise<T> {
		const resp = await request('PUT', path, body);
		if (!resp.ok) throw new Error(`API error: ${resp.status}`);
		return resp.json();
	},

	async del(path: string): Promise<void> {
		const resp = await request('DELETE', path);
		if (!resp.ok) throw new Error(`API error: ${resp.status}`);
	},

	/**
	 * Record that the user just completed login. The actual token lives
	 * in the HttpOnly cookie — this flag only drives client-side routing.
	 * The `_token` argument is kept for call-site compatibility but is
	 * intentionally unused (we never persist it).
	 */
	setToken(_token: string) {
		if (typeof window !== 'undefined') {
			localStorage.setItem(AUTH_FLAG_KEY, '1');
		}
	},

	clearToken() {
		if (typeof window !== 'undefined') {
			localStorage.removeItem(AUTH_FLAG_KEY);
			// Legacy key — clean up in case an older build left one behind.
			localStorage.removeItem('custody_token');
		}
	},

	/**
	 * Server-side logout. Clears the HttpOnly cookie and revokes the
	 * session record in the DB. Falls back to local cleanup if the
	 * network call fails.
	 */
	async logout(): Promise<void> {
		try {
			await request('DELETE', '/v1/auth/sessions/current');
		} catch {
			// Server may be unreachable — the local flag below still
			// kicks the user out on the next route guard check.
		}
		this.clearToken();
	},

	isAuthenticated(): boolean {
		if (typeof window === 'undefined') return false;
		return localStorage.getItem(AUTH_FLAG_KEY) === '1';
	},
};
