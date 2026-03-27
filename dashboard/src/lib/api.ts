/**
 * API client for MG Custody backend.
 *
 * All requests include the session Bearer token from localStorage.
 * Responses are typed JSON.
 */

const BASE_URL = '/';

function getToken(): string | null {
	if (typeof window === 'undefined') return null;
	return localStorage.getItem('custody_token');
}

function headers(): HeadersInit {
	const h: Record<string, string> = {
		'Content-Type': 'application/json',
	};
	const token = getToken();
	if (token) {
		h['Authorization'] = `Bearer ${token}`;
	}
	return h;
}

async function request(method: string, path: string, body?: unknown): Promise<Response> {
	const url = `${BASE_URL}${path.startsWith('/') ? path.slice(1) : path}`;
	const opts: RequestInit = {
		method,
		headers: headers(),
	};
	if (body !== undefined) {
		opts.body = JSON.stringify(body);
	}
	const resp = await fetch(url, opts);
	if (resp.status === 401) {
		// Session expired — clear token and redirect to login
		localStorage.removeItem('custody_token');
		if (typeof window !== 'undefined' && !window.location.pathname.startsWith('/login')) {
			window.location.href = '/login';
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

	setToken(token: string) {
		localStorage.setItem('custody_token', token);
	},

	clearToken() {
		localStorage.removeItem('custody_token');
	},

	isAuthenticated(): boolean {
		return !!getToken();
	},
};
