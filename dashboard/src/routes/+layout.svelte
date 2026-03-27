<script lang="ts">
	let { children } = $props();

	const navItems = [
		{ href: '/', label: 'Dashboard', icon: '~' },
		{ href: '/wallets', label: 'Wallets', icon: 'W' },
		{ href: '/transactions', label: 'Transactions', icon: 'T' },
		{ href: '/approvals', label: 'Approvals', icon: 'A' },
		{ href: '/addresses', label: 'Addresses', icon: '@' },
		{ href: '/policies', label: 'Policies', icon: 'P' },
		{ href: '/users', label: 'Users', icon: 'U' },
		{ href: '/audit', label: 'Audit Log', icon: 'L' },
		{ href: '/settings', label: 'Settings', icon: 'S' },
	];
</script>

<svelte:head>
	<title>MG Custody</title>
</svelte:head>

<div class="app">
	<nav class="sidebar">
		<div class="logo">
			<h2>MG Custody</h2>
		</div>
		<ul>
			{#each navItems as item}
				<li>
					<a href={item.href}>
						<span class="icon">{item.icon}</span>
						{item.label}
					</a>
				</li>
			{/each}
		</ul>
	</nav>
	<main class="content">
		{@render children()}
	</main>
</div>

<style>
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
	}

	.logo {
		padding: 0 1rem 1rem;
		border-bottom: 1px solid var(--border, #30363d);
		margin-bottom: 0.5rem;
	}

	.logo h2 {
		margin: 0;
		font-size: 1.1rem;
		color: var(--accent, #58a6ff);
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
		width: 1.2rem;
		text-align: center;
		font-weight: bold;
		color: var(--accent, #58a6ff);
	}

	.content {
		flex: 1;
		padding: 1.5rem 2rem;
		overflow-y: auto;
	}
</style>
