<script lang="ts">
	import { locale, setLocale, LOCALES } from '$lib/i18n';
	import type { Locale } from '$lib/i18n';
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

	let { children } = $props();

	let sidebarOpen = $state(false);

	// ---------------------------------------------------------------------------
	// User session — role determines which nav items are visible
	// ---------------------------------------------------------------------------

	type UserRole = 'owner' | 'admin' | 'signer' | 'editor' | 'viewer';

	let userEmail = $state('');
	let userRole = $state<UserRole>('viewer');
	let isLoggedIn = $state(false);

	// Roles that can see admin section
	const ADMIN_ROLES: UserRole[] = ['owner', 'admin'];
	// Roles that can see signer-level items (approvals)
	const SIGNER_ROLES: UserRole[] = ['owner', 'admin', 'signer'];

	$effect(() => {
		isLoggedIn = api.isAuthenticated();
	});

	async function loadUserProfile() {
		if (!api.isAuthenticated()) return;
		try {
			const profile = await api.getJson<{ email: string; role: string }>('/v1/user/profile');
			userEmail = profile.email ?? '';
			userRole = (profile.role ?? 'viewer') as UserRole;
		} catch {
			// Not logged in or profile unavailable — defaults are fine
		}
	}

	function logout() {
		api.clearToken();
		userEmail = '';
		userRole = 'viewer';
		isLoggedIn = false;
		window.location.href = '/login';
	}

	// ---------------------------------------------------------------------------
	// Theme management (dark / light)
	// ---------------------------------------------------------------------------

	const THEME_KEY = 'custody_theme';

	let theme = $state<'dark' | 'light'>('dark');

	function applyTheme(t: 'dark' | 'light') {
		if (t === 'light') {
			document.documentElement.classList.add('light');
		} else {
			document.documentElement.classList.remove('light');
		}
		localStorage.setItem(THEME_KEY, t);
		theme = t;
	}

	function toggleTheme() {
		applyTheme(theme === 'dark' ? 'light' : 'dark');
	}

	onMount(() => {
		const stored = localStorage.getItem(THEME_KEY) as 'dark' | 'light' | null;
		applyTheme(stored ?? 'dark');
		loadUserProfile();
	});

	// ---------------------------------------------------------------------------
	// Navigation — role-based visibility
	// ---------------------------------------------------------------------------

	const userItemDefs = [
		{ href: '/',             key: 'nav.portfolio', icon: '$' },
		{ href: '/deposit',      key: 'nav.deposit',   icon: '+' },
		{ href: '/withdraw',     key: 'nav.withdraw',  icon: '-' },
		{ href: '/transactions', key: 'nav.transactions', icon: 'T' },
		{ href: '/approvals',    key: 'nav.approvals', icon: 'A', minRole: 'signer' as UserRole },
		{ href: '/profile',      key: 'nav.profile',   icon: 'Me' },
	];

	const adminItemDefs = [
		{ href: '/wallets',    key: 'nav.wallets',    icon: 'W' },
		{ href: '/policies',   key: 'nav.policies',   icon: 'P' },
		{ href: '/users',      key: 'nav.users',       icon: 'U' },
		{ href: '/addresses',  key: 'nav.addresses',  icon: '@' },
		{ href: '/nfts',       key: 'nav.nfts',       icon: 'N' },
		{ href: '/audit',      key: 'nav.audit',       icon: 'L' },
		{ href: '/webhooks',   key: 'nav.webhooks',   icon: 'W' },
		{ href: '/derivation', key: 'nav.derivation', icon: 'D' },
		{ href: '/emergency',  key: 'nav.emergency',  icon: '!' },
		{ href: '/settings',   key: 'nav.settings',   icon: 'S' },
	];

	function hasRole(minRole: UserRole): boolean {
		const order: UserRole[] = ['viewer', 'editor', 'signer', 'admin', 'owner'];
		return order.indexOf(userRole) >= order.indexOf(minRole);
	}

	function closeSidebar() { sidebarOpen = false; }

	function onLocaleChange(e: Event) {
		setLocale((e.target as HTMLSelectElement).value as Locale);
	}

	import { t } from '$lib/i18n';
</script>

<svelte:head>
	<title>MG Custody</title>
</svelte:head>

<!-- Mobile overlay backdrop -->
{#if sidebarOpen}
	<div
		class="mobile-backdrop"
		role="presentation"
		onclick={closeSidebar}
		onkeydown={(e) => e.key === 'Escape' && closeSidebar()}
	></div>
{/if}

<div class="app">
	<nav class="sidebar" class:open={sidebarOpen}>
		<div class="logo">
			<h2>MG Custody</h2>
			<button
				class="theme-toggle"
				aria-label={theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
				onclick={toggleTheme}
				title={theme === 'dark' ? 'Light mode' : 'Dark mode'}
			>
				{#if theme === 'dark'}
					<!-- Sun icon -->
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
						<circle cx="12" cy="12" r="5"/>
						<line x1="12" y1="1" x2="12" y2="3"/>
						<line x1="12" y1="21" x2="12" y2="23"/>
						<line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/>
						<line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/>
						<line x1="1" y1="12" x2="3" y2="12"/>
						<line x1="21" y1="12" x2="23" y2="12"/>
						<line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/>
						<line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/>
					</svg>
				{:else}
					<!-- Moon icon -->
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
						<path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/>
					</svg>
				{/if}
			</button>
		</div>
		<ul>
			{#each userItemDefs as item}
				{#if !item.minRole || hasRole(item.minRole)}
					<li>
						<a href={item.href} onclick={closeSidebar}>
							<span class="icon">{item.icon}</span>
							{$locale, t(item.key)}
						</a>
					</li>
				{/if}
			{/each}
		</ul>

		{#if hasRole('admin')}
			<div class="section-sep">
				<span>{$locale, t('nav.admin')}</span>
			</div>

			<ul>
				{#each adminItemDefs as item}
					<li>
						<a href={item.href} onclick={closeSidebar}>
							<span class="icon">{item.icon}</span>
							{$locale, t(item.key)}
						</a>
					</li>
				{/each}
			</ul>
		{/if}

		<!-- Language picker -->
		<div class="lang-picker">
			<label class="lang-label" for="lang-select">
				&#127760;
			</label>
			<select
				id="lang-select"
				class="lang-select"
				value={$locale}
				onchange={onLocaleChange}
			>
				{#each LOCALES as loc}
					<option value={loc.code}>{loc.label}</option>
				{/each}
			</select>
		</div>

		<!-- User info + Logout -->
		{#if isLoggedIn}
			<div class="user-footer">
				<div class="user-info">
					<span class="user-email" title={userEmail}>{userEmail || 'User'}</span>
					<span class="role-badge">{userRole}</span>
				</div>
				<button class="logout-btn" onclick={logout} title="Sign out">
					<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
						<path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4"/>
						<polyline points="16 17 21 12 16 7"/>
						<line x1="21" y1="12" x2="9" y2="12"/>
					</svg>
				</button>
			</div>
		{/if}
	</nav>

	<div class="content-wrap">
		<header class="mobile-header">
			<button
				class="hamburger"
				aria-label="Toggle navigation"
				aria-expanded={sidebarOpen}
				onclick={() => { sidebarOpen = !sidebarOpen; }}
			>
				<span></span>
				<span></span>
				<span></span>
			</button>
			<span class="mobile-title">MG Custody</span>
		</header>
		<main class="content">
			{@render children()}
		</main>
	</div>
</div>

<style>
	/* ---------------------------------------------------------------------------
	   CSS custom properties — dark (default) and light palettes.
	   Applied via class on <html>: dark = no class, light = class="light".
	---------------------------------------------------------------------------- */
	:global(:root) {
		--bg:         #0f1117;
		--fg:         #e1e4e8;
		--card-bg:    #161b22;
		--border:     #30363d;
		--fg-muted:   #8b949e;
		--accent:     #58a6ff;
		--hover-bg:   #1c2128;
		--sidebar-bg: #161b22;
		--green:      #3fb950;
		--yellow:     #d29922;
		--red:        #f85149;
	}

	:global(:root.light) {
		--bg:         #ffffff;
		--fg:         #1f2328;
		--card-bg:    #f6f8fa;
		--border:     #d0d7de;
		--fg-muted:   #656d76;
		--accent:     #0969da;
		--hover-bg:   #f3f4f6;
		--sidebar-bg: #f6f8fa;
		--green:      #1a7f37;
		--yellow:     #9a6700;
		--red:        #cf222e;
	}

	:global(body) {
		margin: 0;
		font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
		background: var(--bg, #0f1117);
		color: var(--fg, #e1e4e8);
	}

	.app {
		display: flex;
		min-height: 100vh;
	}

	.sidebar {
		width: 220px;
		background: var(--sidebar-bg, #161b22);
		border-right: 1px solid var(--border, #30363d);
		padding: 1rem 0;
		flex-shrink: 0;
		display: flex;
		flex-direction: column;
	}

	.logo {
		padding: 0 1rem 1rem;
		border-bottom: 1px solid var(--border, #30363d);
		margin-bottom: 0.5rem;
		display: flex;
		align-items: center;
		justify-content: space-between;
	}

	.logo h2 {
		margin: 0;
		font-size: 1.1rem;
		color: var(--accent, #58a6ff);
	}

	.theme-toggle {
		display: flex;
		align-items: center;
		justify-content: center;
		background: none;
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		padding: 4px 6px;
		cursor: pointer;
		color: var(--fg-muted, #8b949e);
		transition: background 0.15s, color 0.15s, border-color 0.15s;
		flex-shrink: 0;
	}

	.theme-toggle:hover {
		background: var(--hover-bg, #1c2128);
		color: var(--fg, #e1e4e8);
		border-color: var(--accent, #58a6ff);
	}

	ul {
		list-style: none;
		padding: 0;
		margin: 0;
	}

	li a {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		padding: 0.6rem 1rem;
		color: var(--fg-muted, #8b949e);
		text-decoration: none;
		font-size: 0.9rem;
		transition: background 0.15s, color 0.15s;
	}

	li a:hover {
		background: var(--hover-bg, #1c2128);
		color: var(--fg, #e1e4e8);
	}

	.icon {
		width: 1.4rem;
		text-align: center;
		font-weight: bold;
		font-size: 0.78rem;
		color: var(--accent, #58a6ff);
	}

	.section-sep {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.9rem 1rem 0.4rem;
		font-size: 0.7rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--fg-muted, #8b949e);
		border-top: 1px solid var(--border, #30363d);
		margin-top: 0.5rem;
	}

	/* Language picker — pinned to sidebar bottom */
	.lang-picker {
		margin-top: auto;
		padding: 0.75rem 1rem;
		border-top: 1px solid var(--border, #30363d);
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}

	.lang-label {
		font-size: 1rem;
		cursor: pointer;
	}

	.lang-select {
		flex: 1;
		background: var(--sidebar-bg, #161b22);
		color: var(--fg-muted, #8b949e);
		border: 1px solid var(--border, #30363d);
		border-radius: 4px;
		padding: 0.3rem 0.4rem;
		font-size: 0.8rem;
		cursor: pointer;
	}

	.lang-select:focus {
		outline: 1px solid var(--accent, #58a6ff);
		color: var(--fg, #e1e4e8);
	}

	/* User info + logout at sidebar bottom */
	.user-footer {
		padding: 0.6rem 1rem;
		border-top: 1px solid var(--border, #30363d);
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 0.5rem;
	}

	.user-info {
		display: flex;
		flex-direction: column;
		gap: 0.15rem;
		min-width: 0;
	}

	.user-email {
		font-size: 0.78rem;
		color: var(--fg, #e1e4e8);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.role-badge {
		font-size: 0.65rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--accent, #58a6ff);
		background: rgba(88, 166, 255, 0.1);
		padding: 0.1rem 0.4rem;
		border-radius: 3px;
		width: fit-content;
	}

	.logout-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		background: none;
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		padding: 6px;
		cursor: pointer;
		color: var(--fg-muted, #8b949e);
		flex-shrink: 0;
		transition: background 0.15s, color 0.15s, border-color 0.15s;
	}

	.logout-btn:hover {
		background: rgba(248, 81, 73, 0.1);
		color: var(--red, #f85149);
		border-color: var(--red, #f85149);
	}

	.content-wrap {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.mobile-header {
		display: none;
		align-items: center;
		gap: 0.75rem;
		padding: 0.75rem 1rem;
		background: var(--sidebar-bg, #161b22);
		border-bottom: 1px solid var(--border, #30363d);
		position: sticky;
		top: 0;
		z-index: 50;
	}

	.mobile-title {
		font-size: 1rem;
		font-weight: 600;
		color: var(--accent, #58a6ff);
	}

	.hamburger {
		display: none;
		flex-direction: column;
		gap: 5px;
		background: none;
		border: none;
		cursor: pointer;
		padding: 4px;
	}

	.hamburger span {
		display: block;
		width: 22px;
		height: 2px;
		background: var(--fg, #e1e4e8);
		border-radius: 2px;
		transition: opacity 0.15s;
	}

	.content {
		flex: 1;
		padding: 1.5rem 2rem;
		overflow-y: auto;
	}

	.mobile-backdrop {
		display: none;
	}

	/* -----------------------------------------------------------------------
	   Mobile breakpoint
	----------------------------------------------------------------------- */
	@media (max-width: 768px) {
		.sidebar {
			position: fixed;
			left: -220px;
			top: 0;
			z-index: 100;
			height: 100vh;
			transition: left 0.2s ease;
			overflow-y: auto;
		}

		.sidebar.open {
			left: 0;
		}

		.mobile-header {
			display: flex;
		}

		.hamburger {
			display: flex;
		}

		.content {
			padding: 1rem;
		}

		.mobile-backdrop {
			display: block;
			position: fixed;
			inset: 0;
			background: rgba(0, 0, 0, 0.5);
			z-index: 99;
		}
	}
</style>
