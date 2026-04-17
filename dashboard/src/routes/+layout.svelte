<script lang="ts">
	import { locale, setLocale, LOCALES, t } from '$lib/i18n';
	import type { Locale } from '$lib/i18n';
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { page } from '$app/stores';
	import { icon } from '$lib/icons/ui';
	import type { IconKey } from '$lib/icons/ui';

	let { children } = $props();

	let sidebarOpen = $state(false);
	let userMenuOpen = $state(false);
	let envMenuOpen = $state(false);

	let isPublicPage = $derived(
		$page.url.pathname.startsWith('/login') ||
		$page.url.pathname.startsWith('/register')
	);

	type UserRole = 'owner' | 'admin' | 'signer' | 'editor' | 'viewer';

	let userEmail = $state('');
	let userRole = $state<UserRole>('viewer');
	let isLoggedIn = $state(false);

	$effect(() => { isLoggedIn = api.isAuthenticated(); });

	async function loadUserProfile() {
		if (!api.isAuthenticated()) return;
		try {
			const profile = await api.getJson<{ email: string; role: string }>('/v1/user/profile');
			userEmail = profile.email ?? '';
			userRole = (profile.role ?? 'viewer') as UserRole;
		} catch { /* ignore */ }
	}

	async function logout() {
		await api.logout();
		userEmail = '';
		userRole = 'viewer';
		isLoggedIn = false;
		window.location.href = '/login';
	}

	const THEME_KEY = 'custody_theme';
	let theme = $state<'dark' | 'light'>('dark');

	function applyTheme(tName: 'dark' | 'light') {
		if (tName === 'light') document.documentElement.classList.add('light');
		else document.documentElement.classList.remove('light');
		localStorage.setItem(THEME_KEY, tName);
		theme = tName;
	}

	function toggleTheme() { applyTheme(theme === 'dark' ? 'light' : 'dark'); }

	onMount(() => {
		const stored = localStorage.getItem(THEME_KEY) as 'dark' | 'light' | null;
		applyTheme(stored ?? 'dark');
		loadUserProfile();

		const onDocClick = (e: MouseEvent) => {
			const target = e.target as HTMLElement;
			if (!target.closest('.user-trigger') && !target.closest('.user-menu')) userMenuOpen = false;
			if (!target.closest('.env-trigger') && !target.closest('.env-menu'))   envMenuOpen  = false;
		};
		const onKey = (e: KeyboardEvent) => {
			if (e.key === 'Escape') { userMenuOpen = false; envMenuOpen = false; sidebarOpen = false; }
		};
		document.addEventListener('click', onDocClick);
		document.addEventListener('keydown', onKey);
		return () => {
			document.removeEventListener('click', onDocClick);
			document.removeEventListener('keydown', onKey);
		};
	});

	type NavItem = { href: string; key: string; icon: IconKey; minRole?: UserRole };

	const userItemDefs: NavItem[] = [
		{ href: '/',             key: 'nav.portfolio',    icon: 'portfolio' },
		{ href: '/deposit',      key: 'nav.deposit',      icon: 'deposit' },
		{ href: '/withdraw',     key: 'nav.withdraw',     icon: 'withdraw' },
		{ href: '/transactions', key: 'nav.transactions', icon: 'transactions' },
		{ href: '/approvals',    key: 'nav.approvals',    icon: 'approvals', minRole: 'signer' as UserRole },
	];

	const adminItemDefs: NavItem[] = [
		{ href: '/wallets',    key: 'nav.wallets',    icon: 'wallets' },
		{ href: '/policies',   key: 'nav.policies',   icon: 'policies' },
		{ href: '/addresses',  key: 'nav.addresses',  icon: 'addresses' },
		{ href: '/tokens',     key: 'nav.tokens',     icon: 'tokens' },
		{ href: '/nfts',       key: 'nav.nfts',       icon: 'nfts' },
		{ href: '/users',      key: 'nav.users',      icon: 'users' },
	];

	const systemItemDefs: NavItem[] = [
		{ href: '/audit',      key: 'nav.audit',      icon: 'audit' },
		{ href: '/webhooks',   key: 'nav.webhooks',   icon: 'webhooks' },
		{ href: '/derivation', key: 'nav.derivation', icon: 'derivation' },
		{ href: '/backup',     key: 'nav.backup',     icon: 'backup' },
		{ href: '/emergency',  key: 'nav.emergency',  icon: 'emergency' },
		{ href: '/settings',   key: 'nav.settings',   icon: 'settings' },
	];

	function hasRole(minRole: UserRole): boolean {
		const order: UserRole[] = ['viewer', 'editor', 'signer', 'admin', 'owner'];
		return order.indexOf(userRole) >= order.indexOf(minRole);
	}

	// ─── Route-level access guard ─────────────────────────────────
	// Every URL has a minimum role. Backend enforces auth via RBAC middleware,
	// but we also guard here so the UI shell doesn't even attempt to render
	// pages a user is not allowed to see.
	const ROUTE_MIN_ROLE: Array<[RegExp, UserRole]> = [
		[/^\/approvals/,  'signer'],
		[/^\/wallets/,    'admin'],
		[/^\/policies/,   'admin'],
		[/^\/addresses/,  'admin'],
		[/^\/tokens/,     'admin'],
		[/^\/nfts/,       'admin'],
		[/^\/users/,      'admin'],
		[/^\/audit/,      'admin'],
		[/^\/webhooks/,   'admin'],
		[/^\/derivation/, 'admin'],
		[/^\/backup/,     'admin'],
		[/^\/emergency/,  'owner'],
		[/^\/settings/,   'admin'],
	];

	function minRoleForPath(path: string): UserRole | null {
		for (const [re, role] of ROUTE_MIN_ROLE) if (re.test(path)) return role;
		return null;
	}

	let accessDenied = $state<{ path: string; required: UserRole } | null>(null);

	// After the profile loads, check if the current route is allowed.
	// We cannot redirect synchronously on first render because userRole is
	// loaded via async profile fetch, so the flow is:
	//   1. user navigates to /users
	//   2. layout mounts, loadUserProfile() sets userRole
	//   3. this $effect detects mismatch and blocks render of page slot
	$effect(() => {
		if (!isLoggedIn || isPublicPage) { accessDenied = null; return; }
		const required = minRoleForPath($page.url.pathname);
		if (required && !hasRole(required)) {
			accessDenied = { path: $page.url.pathname, required };
		} else {
			accessDenied = null;
		}
	});

	function goHome() {
		accessDenied = null;
		if (typeof window !== 'undefined') window.location.href = '/';
	}

	function isActive(href: string): boolean {
		const path = $page.url.pathname;
		if (href === '/') return path === '/';
		return path === href || path.startsWith(href + '/');
	}

	function activeTitle(): string {
		const path = $page.url.pathname;
		const all = [...userItemDefs, ...adminItemDefs, ...systemItemDefs];
		const match = all.find(i => i.href !== '/' && (path === i.href || path.startsWith(i.href + '/')));
		if (match) return t(match.key);
		if (path === '/') return t('nav.portfolio');
		return '';
	}

	function closeSidebar() { sidebarOpen = false; }

	function onLocaleChange(e: Event) {
		setLocale((e.target as HTMLSelectElement).value as Locale);
	}

	function userInitials(email: string): string {
		if (!email) return '·';
		const local = email.split('@')[0] ?? '';
		const parts = local.split(/[._-]/).filter(Boolean);
		if (parts.length >= 2) return (parts[0][0] + parts[1][0]).toUpperCase();
		return local.slice(0, 2).toUpperCase();
	}

	// Environment — live from config later; hardcoded for now
	let env = $state<'production' | 'testnet' | 'sandbox'>('production');
	let envLabels = $derived<Record<'production' | 'testnet' | 'sandbox', { label: string; tone: string }>>(({
		production: { label: ($locale, t('layout.env.production')), tone: 'green'  },
		testnet:    { label: ($locale, t('layout.env.testnet')),    tone: 'yellow' },
		sandbox:    { label: ($locale, t('layout.env.sandbox')),    tone: 'blue'   },
	}));

	// Notification mock
	let notificationCount = $state(0);
</script>

<svelte:head>
	<title>MG Custody</title>
	<link rel="icon" type="image/svg+xml" href="/favicon.svg" />
	<meta name="description" content="Self-hosted cryptocurrency custody — institutional key management, multi-chain, auditable." />
</svelte:head>

{#if sidebarOpen}
	<div
		class="mobile-backdrop"
		role="presentation"
		onclick={closeSidebar}
		onkeydown={(e) => e.key === 'Escape' && closeSidebar()}
	></div>
{/if}

{#if isPublicPage}
	<div class="public-page">
		{@render children()}
	</div>
{:else}
<div class="app">
	<aside class="sidebar" class:open={sidebarOpen}>
		<!-- Workspace switcher -->
		<div class="workspace">
			<button
				class="workspace-btn env-trigger"
				onclick={(e) => { e.stopPropagation(); envMenuOpen = !envMenuOpen; userMenuOpen = false; }}
				aria-haspopup="menu"
				aria-expanded={envMenuOpen}
			>
				<span class="workspace-mark">
					<svg width="22" height="22" viewBox="0 0 32 32" fill="none" xmlns="http://www.w3.org/2000/svg">
						<defs>
							<linearGradient id="brand-g" x1="0" y1="0" x2="32" y2="32" gradientUnits="userSpaceOnUse">
								<stop stop-color="#6FA8FF"/>
								<stop offset="1" stop-color="#3D6BE0"/>
							</linearGradient>
						</defs>
						<path d="M16 3L4 8v9c0 7 4.8 13.2 12 15 7.2-1.8 12-8 12-15V8l-12-5z" fill="url(#brand-g)"/>
						<path d="M12 17l2.8 2.8L21 13.5" stroke="#fff" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
					</svg>
				</span>
				<span class="workspace-body">
					<span class="workspace-name">MG Custody</span>
					<span class="workspace-env">
						<span class="env-dot tone-{envLabels[env].tone}"></span>
						{envLabels[env].label}
					</span>
				</span>
				<span class="workspace-chev" aria-hidden="true">{@html icon('chevron-down', 14)}</span>
			</button>

			{#if envMenuOpen}
				<div class="env-menu popover">
					<div class="menu-section-label">{$locale, t('layout.env.select')}</div>
					{#each Object.entries(envLabels) as [k, v]}
						<button
							class="menu-item"
							class:selected={env === k}
							onclick={() => { env = k as typeof env; envMenuOpen = false; }}
						>
							<span class="env-dot tone-{v.tone}"></span>
							<span>{v.label}</span>
							{#if env === k}
								<span class="menu-check">{@html icon('check', 14)}</span>
							{/if}
						</button>
					{/each}
				</div>
			{/if}
		</div>

		<!-- Command bar trigger (visual only for now) -->
		<button class="cmdbar" aria-label={($locale, t('common.search'))}>
			<span class="cmdbar-icon">{@html icon('search', 14)}</span>
			<span class="cmdbar-label">{$locale, t('layout.search.hint')}</span>
			<kbd class="kbd">⌘K</kbd>
		</button>

		<!-- Nav -->
		<nav class="nav" aria-label="Main navigation">
			<div class="nav-section">
				<span class="nav-section-title">{$locale, t('layout.personal')}</span>
			</div>
			<ul class="nav-list">
				{#each userItemDefs as item}
					{#if !item.minRole || hasRole(item.minRole)}
						<li>
							<a
								href={item.href}
								onclick={closeSidebar}
								class="nav-item"
								class:active={isActive(item.href)}
								aria-current={isActive(item.href) ? 'page' : undefined}
							>
								<span class="nav-icon">{@html icon(item.icon, 16)}</span>
								<span class="nav-label">{$locale, t(item.key)}</span>
								{#if item.icon === 'approvals' && notificationCount > 0}
									<span class="nav-badge">{notificationCount}</span>
								{/if}
							</a>
						</li>
					{/if}
				{/each}
			</ul>

			{#if hasRole('admin')}
				<div class="nav-divider"></div>
				<div class="nav-section with-badge">
					<span class="nav-section-title admin-title">{$locale, t('layout.organization')}</span>
					<span class="admin-chip">
						<span class="admin-dot"></span>
						{$locale, t('layout.admin_badge')}
					</span>
				</div>
				<ul class="nav-list">
					{#each adminItemDefs as item}
						<li>
							<a
								href={item.href}
								onclick={closeSidebar}
								class="nav-item"
								class:active={isActive(item.href)}
							>
								<span class="nav-icon">{@html icon(item.icon, 16)}</span>
								<span class="nav-label">{$locale, t(item.key)}</span>
							</a>
						</li>
					{/each}
				</ul>

				<div class="nav-section">
					<span class="nav-section-title">{$locale, t('layout.system')}</span>
				</div>
				<ul class="nav-list">
					{#each systemItemDefs as item}
						<li>
							<a
								href={item.href}
								onclick={closeSidebar}
								class="nav-item"
								class:active={isActive(item.href)}
							>
								<span class="nav-icon">{@html icon(item.icon, 16)}</span>
								<span class="nav-label">{$locale, t(item.key)}</span>
							</a>
						</li>
					{/each}
				</ul>
			{/if}
		</nav>

	</aside>

	<div class="content-wrap">
		<!-- Top bar -->
		<header class="topbar">
			<div class="topbar-left">
				<button
					class="hamburger"
					aria-label="Toggle navigation"
					aria-expanded={sidebarOpen}
					onclick={() => { sidebarOpen = !sidebarOpen; }}
				>
					{@html icon('menu', 18)}
				</button>
				<div class="crumbs">
					<span class="crumb-root">MG Custody</span>
					<span class="crumb-sep" aria-hidden="true">/</span>
					<span class="crumb-current">{$locale, activeTitle()}</span>
				</div>
			</div>

			<div class="topbar-right">
				<div class="top-env">
					<span class="env-dot tone-{envLabels[env].tone}"></span>
					<span>{envLabels[env].label}</span>
				</div>

				<div class="top-sep"></div>

				<button class="top-btn" aria-label={($locale, t('layout.notifications'))} title={($locale, t('layout.notifications'))}>
					{@html icon('bell', 16)}
					{#if notificationCount > 0}
						<span class="top-badge">{notificationCount}</span>
					{/if}
				</button>

				<button class="top-btn" aria-label={($locale, t('layout.help'))} title={($locale, t('layout.help'))}>
					{@html icon('circle', 16)}
				</button>

				{#if isLoggedIn}
					<div class="user-wrap">
						<button
							class="user-trigger"
							aria-haspopup="menu"
							aria-expanded={userMenuOpen}
							onclick={(e) => { e.stopPropagation(); userMenuOpen = !userMenuOpen; envMenuOpen = false; }}
						>
							<span class="avatar" aria-hidden="true">{userInitials(userEmail)}</span>
							<span class="user-meta">
								<span class="user-email" title={userEmail}>{userEmail || 'User'}</span>
								<span class="user-role">{userRole}</span>
							</span>
							<span class="user-chev" aria-hidden="true">{@html icon('chevron-down', 12)}</span>
						</button>

						{#if userMenuOpen}
							<div class="user-menu popover" role="menu">
								<div class="menu-user-head">
									<span class="avatar lg" aria-hidden="true">{userInitials(userEmail)}</span>
									<div>
										<span class="menu-user-email">{userEmail}</span>
										<span class="menu-user-role">
											<span class="role-dot"></span>
											{userRole}
										</span>
									</div>
								</div>

								<a href="/profile" class="menu-item" onclick={() => userMenuOpen = false}>
									<span class="menu-icon">{@html icon('profile', 14)}</span>
									<span>{$locale, t('nav.profile')}</span>
								</a>
								<a href="/settings" class="menu-item" onclick={() => userMenuOpen = false}>
									<span class="menu-icon">{@html icon('settings', 14)}</span>
									<span>{$locale, t('nav.settings')}</span>
								</a>

								<button class="menu-item" onclick={toggleTheme}>
									<span class="menu-icon">{@html icon(theme === 'dark' ? 'sun' : 'moon', 14)}</span>
									<span>{$locale, t(theme === 'dark' ? 'layout.theme.light' : 'layout.theme.dark')}</span>
								</button>

								<div class="menu-lang">
									<span class="menu-icon">{@html icon('globe', 14)}</span>
									<select aria-label="Language" value={$locale} onchange={onLocaleChange}>
										{#each LOCALES as loc}
											<option value={loc.code}>{loc.label}</option>
										{/each}
									</select>
								</div>

								<div class="menu-sep"></div>

								<button class="menu-item danger" onclick={logout}>
									<span class="menu-icon">{@html icon('logout', 14)}</span>
									<span>{$locale, t('auth.sign_out')}</span>
								</button>
							</div>
						{/if}
					</div>
				{/if}
			</div>
		</header>

		<main class="content">
			{#if accessDenied}
				<div class="denied-wrap">
					<div class="denied-card">
						<span class="denied-icon">{@html icon('lock', 28)}</span>
						<h2>{$locale, t('denied.title')}</h2>
						<p>{$locale, t('denied.text', { role: accessDenied.required })}</p>
						<p class="muted">{$locale, t('denied.current', { role: userRole })}</p>
						<button class="btn-home" onclick={goHome}>
							<span>{@html icon('portfolio', 14)}</span>
							<span>{$locale, t('denied.back')}</span>
						</button>
					</div>
				</div>
			{:else}
				{@render children()}
			{/if}
		</main>
	</div>
</div>
{/if}

<style>
	/* =========================================================================
	   Design tokens
	========================================================================= */
	:global(:root) {
		--bg:           #0A0B0D;
		--bg-elev:      #101114;
		--surface:      #0F1013;
		--surface-2:    #14161B;
		--surface-3:    #1B1E23;
		--sidebar-bg:   #0B0C0F;
		--topbar-bg:    rgba(10, 11, 13, 0.72);

		--border:        #1C1F25;
		--border-strong: #2A2E35;

		--fg:        #E6E8EC;
		--fg-strong: #FFFFFF;
		--fg-muted:  #8A919E;
		--fg-dim:    #5A626F;

		--accent:        #6FA8FF;
		--accent-strong: #82B6FF;
		--accent-soft:   rgba(111, 168, 255, 0.10);
		--accent-line:   rgba(111, 168, 255, 0.22);

		--green:      #34D399;
		--green-soft: rgba(52, 211, 153, 0.12);
		--yellow:     #F5B942;
		--yellow-soft:rgba(245, 185, 66, 0.12);
		--red:        #F87171;
		--red-soft:   rgba(248, 113, 113, 0.12);
		--blue:       #60A5FA;
		--blue-soft:  rgba(96, 165, 250, 0.12);

		--hover-bg:  #15171C;

		/* Back-compat aliases for pages not yet redesigned */
		--card-bg: var(--surface);
		--bg-muted: var(--surface-2);

		--font-ui: 'Inter', ui-sans-serif, system-ui, -apple-system, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;
		--font-mono: 'JetBrains Mono', ui-monospace, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace;

		--sp-1: 0.25rem; --sp-2: 0.5rem; --sp-3: 0.75rem; --sp-4: 1rem;
		--sp-5: 1.25rem; --sp-6: 1.5rem; --sp-8: 2rem; --sp-10: 2.5rem;

		--r-sm: 6px; --r-md: 10px; --r-lg: 14px; --r-xl: 18px; --r-full: 999px;

		--shadow-1: 0 1px 0 rgba(255,255,255,0.03) inset, 0 1px 2px rgba(0,0,0,0.4);
		--shadow-2: 0 1px 0 rgba(255,255,255,0.03) inset, 0 4px 16px rgba(0,0,0,0.4);
		--shadow-pop: 0 1px 0 rgba(255,255,255,0.04) inset, 0 10px 30px rgba(0,0,0,0.5), 0 2px 8px rgba(0,0,0,0.3);
		--shadow-focus: 0 0 0 3px rgba(111, 168, 255, 0.18);

		--sidebar-width: 248px;
		--topbar-height: 52px;
	}

	:global(:root.light) {
		--bg:           #F6F7F9;
		--bg-elev:      #FFFFFF;
		--surface:      #FFFFFF;
		--surface-2:    #F8F9FB;
		--surface-3:    #EFF1F5;
		--sidebar-bg:   #FFFFFF;
		--topbar-bg:    rgba(255, 255, 255, 0.78);

		--border:        #E5E7EB;
		--border-strong: #D0D5DD;

		--fg:        #1A1D21;
		--fg-strong: #0B0C0E;
		--fg-muted:  #5B6472;
		--fg-dim:    #8892A0;

		--accent:        #2563EB;
		--accent-strong: #1D4ED8;
		--accent-soft:   rgba(37, 99, 235, 0.08);
		--accent-line:   rgba(37, 99, 235, 0.22);

		--green: #059669; --green-soft: rgba(5, 150, 105, 0.1);
		--yellow:#B45309; --yellow-soft:rgba(180, 83, 9, 0.1);
		--red:   #DC2626; --red-soft:   rgba(220, 38, 38, 0.1);
		--blue:  #2563EB; --blue-soft:  rgba(37, 99, 235, 0.08);

		--hover-bg: #F1F3F6;

		--shadow-1: 0 1px 2px rgba(16, 24, 40, 0.05);
		--shadow-2: 0 4px 14px rgba(16, 24, 40, 0.08);
		--shadow-pop: 0 8px 24px rgba(16, 24, 40, 0.12), 0 2px 4px rgba(16, 24, 40, 0.06);
	}

	:global(*) { box-sizing: border-box; }

	:global(html), :global(body) {
		margin: 0; padding: 0;
		background: var(--bg); color: var(--fg);
		font-family: var(--font-ui); font-size: 14px; line-height: 1.5;
		-webkit-font-smoothing: antialiased;
		-moz-osx-font-smoothing: grayscale;
		font-feature-settings: 'cv02','cv03','cv04','cv11';
	}

	:global(h1), :global(h2), :global(h3), :global(h4) {
		font-family: var(--font-ui);
		letter-spacing: -0.015em;
		color: var(--fg-strong); font-weight: 600;
	}

	:global(a) { color: var(--accent); text-decoration: none; }
	:global(a:hover) { color: var(--accent-strong); }

	:global(button) { font-family: inherit; }
	:global(input), :global(textarea), :global(select) { font-family: inherit; font-size: 14px; }

	:global(.mono) {
		font-family: var(--font-mono);
		font-feature-settings: 'tnum','zero';
		font-variant-numeric: tabular-nums;
	}

	:global(.token-cell) { display: inline-flex; align-items: center; gap: var(--sp-2); }
	:global(.token-cell svg) { flex-shrink: 0; border-radius: 50%; }

	:global(::-webkit-scrollbar) { width: 10px; height: 10px; }
	:global(::-webkit-scrollbar-track) { background: transparent; }
	:global(::-webkit-scrollbar-thumb) {
		background: var(--border-strong); border-radius: 999px;
		border: 2px solid var(--bg);
	}
	:global(::-webkit-scrollbar-thumb:hover) { background: var(--fg-dim); }

	/* =========================================================================
	   Layout
	========================================================================= */
	.public-page { min-height: 100vh; }

	.app { display: flex; min-height: 100vh; }

	/* ── Sidebar ── */
	.sidebar {
		width: var(--sidebar-width);
		background: var(--sidebar-bg);
		border-right: 1px solid var(--border);
		padding: var(--sp-3);
		flex-shrink: 0;
		display: flex;
		flex-direction: column;
		position: sticky;
		top: 0;
		height: 100vh;
		overflow-y: auto;
		overflow-x: hidden;
	}

	/* Workspace switcher */
	.workspace { position: relative; margin-bottom: var(--sp-3); }

	.workspace-btn {
		display: flex;
		align-items: center;
		gap: var(--sp-2);
		width: 100%;
		padding: 8px 10px;
		background: transparent;
		border: 1px solid transparent;
		border-radius: var(--r-md);
		color: inherit;
		cursor: pointer;
		text-align: left;
		transition: background 0.15s, border-color 0.15s;
	}
	.workspace-btn:hover { background: var(--hover-bg); border-color: var(--border); }

	.workspace-mark {
		display: inline-flex; align-items: center; justify-content: center;
		width: 30px; height: 30px;
		border-radius: 8px;
		background: var(--accent-soft);
		flex-shrink: 0;
	}

	.workspace-body { flex: 1; display: flex; flex-direction: column; min-width: 0; gap: 2px; }

	.workspace-name {
		font-size: 0.86rem; font-weight: 600; color: var(--fg-strong);
		letter-spacing: -0.01em;
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
	}

	.workspace-env {
		font-size: 0.7rem;
		color: var(--fg-muted);
		display: inline-flex;
		align-items: center;
		gap: 5px;
	}

	.env-dot {
		width: 6px; height: 6px; border-radius: 50%;
		background: var(--fg-dim);
	}
	.env-dot.tone-green  { background: var(--green);  box-shadow: 0 0 0 2.5px rgba(52, 211, 153, 0.18); }
	.env-dot.tone-yellow { background: var(--yellow); box-shadow: 0 0 0 2.5px rgba(245, 185, 66, 0.18); }
	.env-dot.tone-blue   { background: var(--blue);   box-shadow: 0 0 0 2.5px rgba(96, 165, 250, 0.18); }

	.workspace-chev { color: var(--fg-dim); display: inline-flex; flex-shrink: 0; }

	/* Popover */
	.popover {
		position: absolute;
		z-index: 50;
		background: var(--surface);
		border: 1px solid var(--border-strong);
		border-radius: var(--r-md);
		box-shadow: var(--shadow-pop);
		min-width: 220px;
		padding: var(--sp-1);
	}

	.env-menu { left: 0; right: 0; top: calc(100% + 6px); }

	.menu-section-label {
		padding: 8px 10px 4px;
		font-size: 0.66rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.1em;
		color: var(--fg-dim);
	}

	.menu-item {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		padding: 7px 10px;
		background: transparent;
		border: none;
		border-radius: var(--r-sm);
		color: var(--fg);
		font-size: 0.85rem;
		text-align: left;
		cursor: pointer;
		transition: background 0.1s;
	}
	.menu-item:hover { background: var(--hover-bg); }
	.menu-item.danger { color: var(--red); }
	.menu-item.danger:hover { background: var(--red-soft); }
	.menu-item.selected { color: var(--fg-strong); }
	.menu-check { margin-left: auto; color: var(--accent); display: inline-flex; }

	/* Command bar */
	.cmdbar {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 7px 10px;
		margin-bottom: var(--sp-3);
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		color: var(--fg-muted);
		font-size: 0.8rem;
		cursor: pointer;
		transition: border-color 0.15s, background 0.15s;
	}
	.cmdbar:hover { border-color: var(--border-strong); background: var(--surface-3); color: var(--fg); }
	.cmdbar-icon { display: inline-flex; color: var(--fg-dim); }
	.cmdbar-label { flex: 1; text-align: left; }

	.kbd {
		font-family: var(--font-mono);
		font-size: 0.68rem;
		padding: 2px 6px;
		background: var(--surface);
		border: 1px solid var(--border);
		border-bottom-width: 2px;
		border-radius: 4px;
		color: var(--fg-muted);
	}

	/* Nav */
	.nav {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow-y: auto;
	}

	.nav-list {
		list-style: none; padding: 0; margin: 0;
		display: flex; flex-direction: column; gap: 1px;
	}

	.nav-section {
		padding: var(--sp-4) var(--sp-2) var(--sp-1);
		display: flex;
		align-items: center;
		justify-content: space-between;
	}
	.nav-section:first-of-type { padding-top: var(--sp-2); }

	.nav-section-title {
		font-size: 0.64rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.12em;
		color: var(--fg-dim);
	}
	.nav-section-title.admin-title { color: var(--accent); }

	.nav-divider {
		height: 1px;
		background: var(--border);
		margin: var(--sp-3) var(--sp-2) var(--sp-1);
	}

	.admin-chip {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 0.6rem;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		padding: 2px 7px;
		border-radius: var(--r-full);
		background: var(--accent-soft);
		color: var(--accent);
		border: 1px solid var(--accent-line);
	}
	.admin-dot {
		width: 5px; height: 5px;
		border-radius: 50%;
		background: currentColor;
	}

	.nav-item {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 6px 10px;
		border-radius: var(--r-sm);
		color: var(--fg-muted);
		font-size: 0.83rem;
		font-weight: 500;
		text-decoration: none;
		transition: background 0.15s, color 0.15s;
		position: relative;
	}

	.nav-item:hover {
		background: var(--hover-bg);
		color: var(--fg);
	}

	.nav-item.active {
		background: var(--accent-soft);
		color: var(--fg-strong);
	}

	.nav-item.active::after {
		content: '';
		position: absolute;
		left: 0;
		top: 7px;
		bottom: 7px;
		width: 2px;
		background: var(--accent);
		border-radius: 0 2px 2px 0;
	}

	.nav-icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 16px; height: 16px;
		color: var(--fg-dim);
		flex-shrink: 0;
	}

	.nav-item:hover .nav-icon,
	.nav-item.active .nav-icon { color: var(--accent); }

	.nav-label {
		flex: 1;
		overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
	}

	.nav-badge {
		font-size: 0.68rem;
		font-weight: 600;
		color: #0A0B0D;
		background: var(--yellow);
		padding: 1px 6px;
		border-radius: var(--r-full);
		min-width: 18px;
		text-align: center;
	}

	/* User section (in topbar) */
	.user-wrap { position: relative; }

	.user-trigger {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 4px 8px 4px 4px;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: var(--r-full);
		color: inherit;
		cursor: pointer;
		text-align: left;
		transition: background 0.15s, border-color 0.15s;
	}
	.user-trigger:hover { background: var(--hover-bg); border-color: var(--border-strong); }

	.avatar {
		width: 26px; height: 26px;
		border-radius: 50%;
		background: linear-gradient(135deg, var(--accent) 0%, #3D6BE0 100%);
		color: #0A0B0D;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 0.64rem;
		font-weight: 700;
		letter-spacing: 0.02em;
		flex-shrink: 0;
	}
	.avatar.lg { width: 40px; height: 40px; font-size: 0.85rem; }

	.user-meta {
		display: flex; flex-direction: column; min-width: 0; gap: 0;
		line-height: 1.1;
		max-width: 140px;
	}

	.user-email {
		font-size: 0.78rem;
		font-weight: 500;
		color: var(--fg);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.user-role {
		font-size: 0.64rem;
		color: var(--fg-muted);
		text-transform: capitalize;
		font-weight: 500;
	}

	.role-dot {
		width: 5px; height: 5px; border-radius: 50%;
		background: var(--accent);
		box-shadow: 0 0 0 2px var(--accent-soft);
		display: inline-block;
	}

	.user-chev { color: var(--fg-dim); display: inline-flex; flex-shrink: 0; }

	.user-menu {
		right: 0;
		top: calc(100% + 6px);
		min-width: 240px;
	}

	.menu-user-head {
		display: flex; align-items: center; gap: 10px;
		padding: 10px;
		border-bottom: 1px solid var(--border);
		margin: -4px -4px 4px;
		background: var(--surface-2);
		border-radius: var(--r-md) var(--r-md) 0 0;
	}

	.menu-user-head > div { display: flex; flex-direction: column; min-width: 0; gap: 2px; }

	.menu-user-email {
		font-size: 0.82rem;
		font-weight: 500;
		color: var(--fg-strong);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.menu-user-role {
		font-size: 0.7rem;
		color: var(--fg-muted);
		text-transform: capitalize;
		font-weight: 500;
		display: inline-flex;
		align-items: center;
		gap: 5px;
	}

	.menu-icon { color: var(--fg-dim); display: inline-flex; flex-shrink: 0; }
	.menu-item:hover .menu-icon { color: var(--fg); }
	.menu-item.danger .menu-icon { color: var(--red); }

	.menu-lang {
		display: flex; align-items: center; gap: 10px;
		padding: 6px 10px;
	}
	.menu-lang select {
		flex: 1;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		color: var(--fg);
		font-size: 0.82rem;
		padding: 4px 6px;
		cursor: pointer;
		outline: none;
	}

	.menu-sep {
		height: 1px;
		background: var(--border);
		margin: 4px 0;
	}

	/* ── Content area ── */
	.content-wrap {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-width: 0;
		background: var(--bg);
	}

	/* Topbar */
	.topbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--sp-4);
		height: var(--topbar-height);
		padding: 0 var(--sp-6);
		border-bottom: 1px solid var(--border);
		background: var(--topbar-bg);
		backdrop-filter: blur(12px);
		-webkit-backdrop-filter: blur(12px);
		position: sticky;
		top: 0;
		z-index: 40;
	}

	.topbar-left { display: flex; align-items: center; gap: var(--sp-3); min-width: 0; }
	.topbar-right { display: flex; align-items: center; gap: var(--sp-2); }

	.hamburger {
		display: none;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		padding: 5px;
		color: var(--fg-muted);
		cursor: pointer;
	}
	.hamburger:hover { color: var(--fg); border-color: var(--border-strong); }

	.crumbs {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 0.84rem;
		min-width: 0;
	}
	.crumb-root { color: var(--fg-muted); }
	.crumb-sep { color: var(--fg-dim); }
	.crumb-current { color: var(--fg-strong); font-weight: 500; }

	.top-btn {
		position: relative;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 32px; height: 32px;
		background: transparent;
		border: 1px solid transparent;
		border-radius: var(--r-sm);
		color: var(--fg-muted);
		cursor: pointer;
		transition: background 0.15s, color 0.15s, border-color 0.15s;
	}
	.top-btn:hover { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }

	.top-badge {
		position: absolute;
		top: 4px; right: 4px;
		min-width: 14px; height: 14px;
		padding: 0 4px;
		font-size: 0.62rem;
		font-weight: 700;
		background: var(--red);
		color: #fff;
		border-radius: var(--r-full);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		line-height: 1;
	}

	.top-env {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 5px 10px;
		font-size: 0.74rem;
		font-weight: 500;
		color: var(--fg-muted);
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-full);
	}

	.top-sep {
		width: 1px;
		height: 20px;
		background: var(--border);
		margin: 0 4px;
	}

	.content {
		flex: 1;
		padding: var(--sp-8) var(--sp-6);
		overflow-y: auto;
		max-width: 1400px;
		width: 100%;
		margin: 0 auto;
	}

	/* Access-denied screen */
	.denied-wrap {
		min-height: 60vh;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: var(--sp-5);
	}
	.denied-card {
		max-width: 440px;
		text-align: center;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
		padding: var(--sp-8) var(--sp-6);
		box-shadow: var(--shadow-1);
	}
	.denied-icon {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		width: 64px; height: 64px;
		border-radius: var(--r-lg);
		background: var(--red-soft);
		color: var(--red);
		margin-bottom: var(--sp-4);
	}
	.denied-card h2 {
		margin: 0 0 var(--sp-2);
		font-size: 1.3rem;
		font-weight: 650;
		color: var(--fg-strong);
		letter-spacing: -0.02em;
	}
	.denied-card p { margin: 0 0 var(--sp-2); color: var(--fg); font-size: 0.9rem; }
	.denied-card p.muted { color: var(--fg-muted); font-size: 0.85rem; }
	.denied-card .role-chip {
		display: inline-block;
		padding: 2px 9px;
		background: var(--accent-soft);
		color: var(--accent);
		border: 1px solid var(--accent-line);
		border-radius: var(--r-full);
		font-size: 0.78rem;
		font-weight: 600;
		text-transform: capitalize;
	}
	.btn-home {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		margin-top: var(--sp-4);
		padding: 10px 16px;
		background: var(--accent);
		color: #0A0B0D;
		border: none;
		border-radius: var(--r-md);
		font-size: 0.88rem;
		font-weight: 600;
		cursor: pointer;
	}
	.btn-home:hover { background: var(--accent-strong); }

	.mobile-backdrop { display: none; }

	@media (max-width: 900px) {
		.sidebar {
			position: fixed;
			left: calc(var(--sidebar-width) * -1 - 8px);
			top: 0;
			z-index: 100;
			height: 100vh;
			transition: left 0.25s ease;
			box-shadow: var(--shadow-2);
		}
		.sidebar.open { left: 0; }

		.hamburger { display: inline-flex; }
		.crumbs { font-size: 0.82rem; }
		.crumb-root, .crumb-sep { display: none; }

		.content { padding: var(--sp-5) var(--sp-4); }

		.mobile-backdrop {
			display: block;
			position: fixed;
			inset: 0;
			background: rgba(0, 0, 0, 0.6);
			backdrop-filter: blur(4px);
			z-index: 99;
		}

		.top-env { display: none; }
		.top-sep { display: none; }
		.user-meta { display: none; }
		.user-chev { display: none; }
		.user-trigger { padding: 2px; border-color: transparent; }
	}
</style>
