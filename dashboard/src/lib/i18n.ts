/**
 * i18n — multi-language support for the MG Custody dashboard.
 *
 * Locale state is stored in a Svelte writable store so all components
 * react to locale changes without a page reload.
 *
 * Usage:
 *   import { t, setLocale, getLocale, locale } from '$lib/i18n';
 *   $: label = t('nav.portfolio');   // reactive in Svelte templates
 *   setLocale('ru');
 */

import { writable, get } from 'svelte/store';

export type Locale = 'en' | 'ru' | 'zh';

// ---------------------------------------------------------------------------
// Translation tables
// ---------------------------------------------------------------------------

const translations: Record<Locale, Record<string, string>> = {
  en: {
    // Navigation
    'nav.portfolio':    'Portfolio',
    'nav.deposit':      'Deposit',
    'nav.withdraw':     'Withdraw',
    'nav.transactions': 'Transactions',
    'nav.approvals':    'Approvals',
    'nav.profile':      'Profile',
    'nav.wallets':      'Wallets',
    'nav.policies':     'Policies',
    'nav.users':        'Users',
    'nav.addresses':    'Addresses',
    'nav.nfts':         'NFTs',
    'nav.audit':        'Audit Log',
    'nav.webhooks':     'Webhooks',
    'nav.derivation':   'Derivation',
    'nav.emergency':    'Emergency',
    'nav.settings':     'Settings',
    'nav.admin':        'Admin',

    // Portfolio page
    'portfolio.title':           'Portfolio',
    'portfolio.total':           'Total Balance',
    'portfolio.assets':          'Assets',
    'portfolio.activity':        'Recent Activity',
    'portfolio.no_balances':     'No balances yet. Make a deposit to get started.',
    'portfolio.no_activity':     'No recent activity.',
    'portfolio.deposit':         'Deposit',
    'portfolio.withdraw':        'Withdraw',
    'portfolio.approvals':       'Approvals',
    'portfolio.col_asset':       'Asset',
    'portfolio.col_balance':     'Balance',
    'portfolio.col_usd':         'USD Value',
    'portfolio.col_pct':         '% of Portfolio',

    // Deposit page
    'deposit.title':        'Deposit',
    'deposit.select_chain': 'Select Chain',
    'deposit.address':      'Deposit Address',
    'deposit.history':      'Deposit History',
    'deposit.qr_hint':      'Scan QR code or copy address below',
    'deposit.memo_hint':    'Include memo if required by the chain',

    // Withdraw page
    'withdraw.title':       'Withdraw',
    'withdraw.amount':      'Amount',
    'withdraw.destination': 'Destination Address',
    'withdraw.estimate':    'Estimate Fee',
    'withdraw.submit':      'Submit Withdrawal',
    'withdraw.asset':       'Asset',
    'withdraw.chain':       'Chain',
    'withdraw.fee':         'Network Fee',

    // Transactions
    'tx.title':      'Transactions',
    'tx.type':       'Type',
    'tx.asset':      'Asset',
    'tx.amount':     'Amount',
    'tx.status':     'Status',
    'tx.date':       'Date',
    'tx.hash':       'Transaction Hash',
    'tx.no_txs':     'No transactions yet.',

    // Approvals
    'approvals.title':    'Pending Approvals',
    'approvals.approve':  'Approve',
    'approvals.reject':   'Reject',
    'approvals.no_items': 'No pending approvals.',
    'approvals.amount':   'Amount',
    'approvals.to':       'To Address',
    'approvals.tier':     'Approval Tier',

    // Profile
    'profile.title':           'Profile',
    'profile.email':           'Email',
    'profile.role':            'Role',
    'profile.mfa':             'Two-Factor Authentication',
    'profile.mfa_enabled':     'Enabled',
    'profile.mfa_disabled':    'Disabled',
    'profile.change_password': 'Change Password',
    'profile.login_history':   'Login History',
    'profile.devices':         'Registered Devices',

    // Wallets (admin)
    'wallets.title':     'Wallet Management',
    'wallets.hot':       'Hot Wallet',
    'wallets.warm':      'Warm Wallet',
    'wallets.cold':      'Cold Wallet',
    'wallets.balance':   'Balance',
    'wallets.target':    'Target Allocation',
    'wallets.sweep':     'Trigger Sweep',

    // Policies (admin)
    'policies.title':   'Access Policies',
    'policies.add':     'Add Policy',
    'policies.edit':    'Edit',
    'policies.delete':  'Delete',
    'policies.name':    'Policy Name',
    'policies.effect':  'Effect',
    'policies.allow':   'Allow',
    'policies.deny':    'Deny',

    // Users (admin)
    'users.title':        'Users',
    'users.invite':       'Invite User',
    'users.email':        'Email',
    'users.role':         'Role',
    'users.status':       'Status',
    'users.active':       'Active',
    'users.disabled':     'Disabled',
    'users.last_login':   'Last Login',

    // Audit log (admin)
    'audit.title':    'Audit Log',
    'audit.event':    'Event',
    'audit.user':     'User',
    'audit.ip':       'IP Address',
    'audit.date':     'Date',
    'audit.no_items': 'No audit events.',

    // Settings (admin)
    'settings.title':        'Settings',
    'settings.chain_nodes':  'Chain Node URLs',
    'settings.webhooks':     'Webhook Configuration',
    'settings.limits':       'Withdrawal Limits',
    'settings.save':         'Save Changes',

    // Common
    'common.loading':   'Loading...',
    'common.error':     'Error',
    'common.copy':      'Copy',
    'common.copied':    'Copied!',
    'common.back':      'Back',
    'common.cancel':    'Cancel',
    'common.confirm':   'Confirm',
    'common.submit':    'Submit',
    'common.search':    'Search',
    'common.filter':    'Filter',
    'common.export':    'Export',
    'common.refresh':   'Refresh',
    'common.no_data':   'No data available.',
    'common.unknown':   'Unknown',

    // Language picker
    'lang.en': 'English',
    'lang.ru': 'Russian',
    'lang.zh': 'Chinese',
  },

  ru: {
    // Navigation
    'nav.portfolio':    'Портфель',
    'nav.deposit':      'Пополнение',
    'nav.withdraw':     'Вывод',
    'nav.transactions': 'Транзакции',
    'nav.approvals':    'Подтверждения',
    'nav.profile':      'Профиль',
    'nav.wallets':      'Кошельки',
    'nav.policies':     'Политики',
    'nav.users':        'Пользователи',
    'nav.addresses':    'Адреса',
    'nav.nfts':         'NFT',
    'nav.audit':        'Журнал аудита',
    'nav.webhooks':     'Вебхуки',
    'nav.derivation':   'Деривация',
    'nav.emergency':    'Экстренное',
    'nav.settings':     'Настройки',
    'nav.admin':        'Администрирование',

    // Portfolio page
    'portfolio.title':           'Портфель',
    'portfolio.total':           'Общий баланс',
    'portfolio.assets':          'Активы',
    'portfolio.activity':        'Последняя активность',
    'portfolio.no_balances':     'Балансов нет. Сделайте пополнение для начала работы.',
    'portfolio.no_activity':     'Нет последних операций.',
    'portfolio.deposit':         'Пополнить',
    'portfolio.withdraw':        'Вывести',
    'portfolio.approvals':       'Подтверждения',
    'portfolio.col_asset':       'Актив',
    'portfolio.col_balance':     'Баланс',
    'portfolio.col_usd':         'USD',
    'portfolio.col_pct':         '% портфеля',

    // Deposit page
    'deposit.title':        'Пополнение',
    'deposit.select_chain': 'Выберите сеть',
    'deposit.address':      'Адрес для пополнения',
    'deposit.history':      'История пополнений',
    'deposit.qr_hint':      'Отсканируйте QR-код или скопируйте адрес ниже',
    'deposit.memo_hint':    'Укажите мемо, если требуется сетью',

    // Withdraw page
    'withdraw.title':       'Вывод средств',
    'withdraw.amount':      'Сумма',
    'withdraw.destination': 'Адрес получателя',
    'withdraw.estimate':    'Рассчитать комиссию',
    'withdraw.submit':      'Отправить вывод',
    'withdraw.asset':       'Актив',
    'withdraw.chain':       'Сеть',
    'withdraw.fee':         'Комиссия сети',

    // Transactions
    'tx.title':      'Транзакции',
    'tx.type':       'Тип',
    'tx.asset':      'Актив',
    'tx.amount':     'Сумма',
    'tx.status':     'Статус',
    'tx.date':       'Дата',
    'tx.hash':       'Хеш транзакции',
    'tx.no_txs':     'Транзакций пока нет.',

    // Approvals
    'approvals.title':    'Ожидают подтверждения',
    'approvals.approve':  'Подтвердить',
    'approvals.reject':   'Отклонить',
    'approvals.no_items': 'Нет ожидающих подтверждений.',
    'approvals.amount':   'Сумма',
    'approvals.to':       'Адрес получателя',
    'approvals.tier':     'Уровень подтверждения',

    // Profile
    'profile.title':           'Профиль',
    'profile.email':           'Email',
    'profile.role':            'Роль',
    'profile.mfa':             'Двухфакторная аутентификация',
    'profile.mfa_enabled':     'Включена',
    'profile.mfa_disabled':    'Выключена',
    'profile.change_password': 'Изменить пароль',
    'profile.login_history':   'История входов',
    'profile.devices':         'Зарегистрированные устройства',

    // Wallets (admin)
    'wallets.title':     'Управление кошельками',
    'wallets.hot':       'Горячий кошелёк',
    'wallets.warm':      'Тёплый кошелёк',
    'wallets.cold':      'Холодный кошелёк',
    'wallets.balance':   'Баланс',
    'wallets.target':    'Целевое распределение',
    'wallets.sweep':     'Выполнить перемещение',

    // Policies (admin)
    'policies.title':   'Политики доступа',
    'policies.add':     'Добавить политику',
    'policies.edit':    'Редактировать',
    'policies.delete':  'Удалить',
    'policies.name':    'Название политики',
    'policies.effect':  'Эффект',
    'policies.allow':   'Разрешить',
    'policies.deny':    'Запретить',

    // Users (admin)
    'users.title':        'Пользователи',
    'users.invite':       'Пригласить пользователя',
    'users.email':        'Email',
    'users.role':         'Роль',
    'users.status':       'Статус',
    'users.active':       'Активен',
    'users.disabled':     'Отключён',
    'users.last_login':   'Последний вход',

    // Audit log (admin)
    'audit.title':    'Журнал аудита',
    'audit.event':    'Событие',
    'audit.user':     'Пользователь',
    'audit.ip':       'IP адрес',
    'audit.date':     'Дата',
    'audit.no_items': 'Событий аудита нет.',

    // Settings (admin)
    'settings.title':        'Настройки',
    'settings.chain_nodes':  'URL узлов блокчейна',
    'settings.webhooks':     'Настройка вебхуков',
    'settings.limits':       'Лимиты на вывод',
    'settings.save':         'Сохранить изменения',

    // Common
    'common.loading':   'Загрузка...',
    'common.error':     'Ошибка',
    'common.copy':      'Копировать',
    'common.copied':    'Скопировано!',
    'common.back':      'Назад',
    'common.cancel':    'Отмена',
    'common.confirm':   'Подтвердить',
    'common.submit':    'Отправить',
    'common.search':    'Поиск',
    'common.filter':    'Фильтр',
    'common.export':    'Экспорт',
    'common.refresh':   'Обновить',
    'common.no_data':   'Данных нет.',
    'common.unknown':   'Неизвестно',

    // Language picker
    'lang.en': 'Английский',
    'lang.ru': 'Русский',
    'lang.zh': 'Китайский',
  },

  zh: {
    // Navigation
    'nav.portfolio':    '资产组合',
    'nav.deposit':      '充值',
    'nav.withdraw':     '提现',
    'nav.transactions': '交易记录',
    'nav.approvals':    '审批',
    'nav.profile':      '个人资料',
    'nav.wallets':      '钱包',
    'nav.policies':     '策略',
    'nav.users':        '用户',
    'nav.addresses':    '地址',
    'nav.nfts':         'NFT',
    'nav.audit':        '审计日志',
    'nav.webhooks':     'Webhooks',
    'nav.derivation':   '派生路径',
    'nav.emergency':    '紧急控制',
    'nav.settings':     '设置',
    'nav.admin':        '管理',

    // Portfolio page
    'portfolio.title':           '资产组合',
    'portfolio.total':           '总余额',
    'portfolio.assets':          '资产',
    'portfolio.activity':        '最近活动',
    'portfolio.no_balances':     '暂无余额，请先充值。',
    'portfolio.no_activity':     '暂无最近活动。',
    'portfolio.deposit':         '充值',
    'portfolio.withdraw':        '提现',
    'portfolio.approvals':       '审批',
    'portfolio.col_asset':       '资产',
    'portfolio.col_balance':     '余额',
    'portfolio.col_usd':         '美元价值',
    'portfolio.col_pct':         '占比',

    // Deposit page
    'deposit.title':        '充值',
    'deposit.select_chain': '选择链',
    'deposit.address':      '充值地址',
    'deposit.history':      '充值历史',
    'deposit.qr_hint':      '扫描二维码或复制以下地址',
    'deposit.memo_hint':    '如链要求，请填写备注',

    // Withdraw page
    'withdraw.title':       '提现',
    'withdraw.amount':      '金额',
    'withdraw.destination': '目标地址',
    'withdraw.estimate':    '估算手续费',
    'withdraw.submit':      '提交提现',
    'withdraw.asset':       '资产',
    'withdraw.chain':       '链',
    'withdraw.fee':         '网络手续费',

    // Transactions
    'tx.title':      '交易记录',
    'tx.type':       '类型',
    'tx.asset':      '资产',
    'tx.amount':     '金额',
    'tx.status':     '状态',
    'tx.date':       '日期',
    'tx.hash':       '交易哈希',
    'tx.no_txs':     '暂无交易记录。',

    // Approvals
    'approvals.title':    '待审批',
    'approvals.approve':  '批准',
    'approvals.reject':   '拒绝',
    'approvals.no_items': '没有待审批项目。',
    'approvals.amount':   '金额',
    'approvals.to':       '目标地址',
    'approvals.tier':     '审批级别',

    // Profile
    'profile.title':           '个人资料',
    'profile.email':           '邮箱',
    'profile.role':            '角色',
    'profile.mfa':             '双因素认证',
    'profile.mfa_enabled':     '已启用',
    'profile.mfa_disabled':    '未启用',
    'profile.change_password': '修改密码',
    'profile.login_history':   '登录历史',
    'profile.devices':         '已注册设备',

    // Wallets (admin)
    'wallets.title':     '钱包管理',
    'wallets.hot':       '热钱包',
    'wallets.warm':      '温钱包',
    'wallets.cold':      '冷钱包',
    'wallets.balance':   '余额',
    'wallets.target':    '目标配比',
    'wallets.sweep':     '触发归集',

    // Policies (admin)
    'policies.title':   '访问策略',
    'policies.add':     '添加策略',
    'policies.edit':    '编辑',
    'policies.delete':  '删除',
    'policies.name':    '策略名称',
    'policies.effect':  '效果',
    'policies.allow':   '允许',
    'policies.deny':    '拒绝',

    // Users (admin)
    'users.title':        '用户',
    'users.invite':       '邀请用户',
    'users.email':        '邮箱',
    'users.role':         '角色',
    'users.status':       '状态',
    'users.active':       '活跃',
    'users.disabled':     '已禁用',
    'users.last_login':   '最后登录',

    // Audit log (admin)
    'audit.title':    '审计日志',
    'audit.event':    '事件',
    'audit.user':     '用户',
    'audit.ip':       'IP 地址',
    'audit.date':     '日期',
    'audit.no_items': '暂无审计事件。',

    // Settings (admin)
    'settings.title':        '设置',
    'settings.chain_nodes':  '节点 URL',
    'settings.webhooks':     'Webhook 配置',
    'settings.limits':       '提现限额',
    'settings.save':         '保存更改',

    // Common
    'common.loading':   '加载中...',
    'common.error':     '错误',
    'common.copy':      '复制',
    'common.copied':    '已复制！',
    'common.back':      '返回',
    'common.cancel':    '取消',
    'common.confirm':   '确认',
    'common.submit':    '提交',
    'common.search':    '搜索',
    'common.filter':    '过滤',
    'common.export':    '导出',
    'common.refresh':   '刷新',
    'common.no_data':   '暂无数据。',
    'common.unknown':   '未知',

    // Language picker
    'lang.en': '英语',
    'lang.ru': '俄语',
    'lang.zh': '中文',
  },
};

// ---------------------------------------------------------------------------
// Locale store
// ---------------------------------------------------------------------------

const STORAGE_KEY = 'mg_custody_locale';

function detectInitialLocale(): Locale {
  if (typeof localStorage !== 'undefined') {
    const stored = localStorage.getItem(STORAGE_KEY) as Locale | null;
    if (stored && stored in translations) {
      return stored;
    }
  }
  // Browser language hint
  if (typeof navigator !== 'undefined') {
    const lang = navigator.language.toLowerCase();
    if (lang.startsWith('ru')) return 'ru';
    if (lang.startsWith('zh')) return 'zh';
  }
  return 'en';
}

/** Svelte writable store for the active locale. Subscribe to re-render on change. */
export const locale = writable<Locale>(detectInitialLocale());

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/**
 * Get the translation for `key` in the currently active locale.
 * Falls back to the English translation if the key is missing.
 * Returns the key itself if no translation exists in any locale.
 */
export function t(key: string): string {
  const current = get(locale);
  return (
    translations[current][key] ??
    translations['en'][key] ??
    key
  );
}

/**
 * Change the active locale. Persists to localStorage.
 * All reactive `$locale` subscriptions will update.
 */
export function setLocale(next: Locale): void {
  locale.set(next);
  if (typeof localStorage !== 'undefined') {
    localStorage.setItem(STORAGE_KEY, next);
  }
}

/** Get the currently active locale string. */
export function getLocale(): Locale {
  return get(locale);
}

/** All available locales with display names in the target language. */
export const LOCALES: { code: Locale; label: string }[] = [
  { code: 'en', label: 'English' },
  { code: 'ru', label: 'Русский' },
  { code: 'zh', label: '中文' },
];
