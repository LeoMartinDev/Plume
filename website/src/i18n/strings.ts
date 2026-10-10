export type Lang = 'en' | 'fr';

export const languages: Record<Lang, string> = { en: 'English', fr: 'Français' };

const en = {
  meta: {
    title: 'Plume — Speak. Release. Keep writing.',
    description:
      'Push-to-talk dictation for macOS, Windows and Linux. Hold a shortcut, speak, let go: Plume types your words into any app. Local speech models, no account, no audio upload.',
  },
  nav: {
    features: 'Features',
    models: 'Models',
    privacy: 'Privacy',
    faq: 'FAQ',
    download: 'Download',
    theme: 'Toggle dark mode',
    menu: 'Main navigation',
  },
  hero: {
    badge: 'Runs 100% on your computer · macOS, Windows & Linux',
    title1: 'Speak. Release.',
    title2: 'Keep writing.',
    lead: "Hold a shortcut, say what you mean, let go. Plume types your words into whatever app you're in — privately, with no account and no audio upload.",
    download: 'Download Plume',
    downloadFor: 'Download for',
    source: 'View source on GitHub',
    hold: 'Hold',
    toTalk: 'to talk',
    windowTitle: 'Notes — Product sync',
    docTitle: 'Product sync',
    docLine1: 'Ship the onboarding flow behind a flag first, then open it to everyone once the copy is final.',
    docLine2:
      "Let's also add a short video on the download page so people see how the shortcut works before they install it",
  },
  pill: {
    ready: 'Ready',
    listening: 'Listening',
    transcribing: 'Transcribing on-device',
    inserted: 'Inserted',
  },
  steps: [
    { title: 'Hold the shortcut', text: 'Wherever your cursor is — a message, a doc, a prompt. Plume starts listening.' },
    { title: 'Say it naturally', text: 'Talk the way you think. Prefer hands-free? Press Ctrl + Shift + Space to start and stop.' },
    { title: "Release. It's typed.", text: 'Your text lands right where you were writing. Changed your mind mid-sentence? Esc cancels.' },
  ],
  features: {
    eyebrow: 'Features',
    title1: 'Small app.',
    title2: 'Out of your way.',
    anyApp: {
      title: 'Works in every app',
      text: 'Messages, notes, documents, AI prompts. Insert automatically, by clipboard paste or by typing — with an optional clipboard fallback.',
      chips: ['Slack', 'Mail', 'Notion', 'VS Code', 'Claude', 'Google Docs'],
      chipLast: 'any text field',
      demos: [
        { app: 'Slack', context: '# design', text: 'Looks great, ship it after lunch.' },
        { app: 'Mail', context: 'Re: Q3 planning', text: 'Thanks Sam, Thursday works for me.' },
        { app: 'VS Code', context: 'retry.rs', text: '// Retry once before showing the error' },
        { app: 'Claude', context: 'New chat', text: 'Summarize this thread in three bullet points.' },
      ],
    },
    items: [
      { icon: 'local', title: 'Fully local', text: 'Speech is transcribed on your machine. Internet is only used to download models and check for updates.' },
      { icon: '99', title: 'Languages', text: 'Whisper models understand 99 languages. Let Plume detect it, or pin French or English.' },
      { icon: 'history', title: 'Never lose a take', text: 'Local history with retention controls. Copy, delete, or recover your eight latest recordings.' },
      { icon: 'theme', title: 'Lives in your menu bar', text: 'Quiet in the menu bar or system tray. Light, dark, or follows your system.' },
      { icon: 'bolt', title: 'Native & fast', text: 'Built in Rust with GPUI. No Electron, no browser tab hiding in the background.' },
    ],
  },
  models: {
    eyebrow: 'Models',
    title: 'Pick your trade-off.',
    lead: 'Download a model once from inside the app. Switch anytime. Everything runs offline after that.',
    head: ['Model', 'Download', 'Languages', 'Best for'],
    rows: [
      { name: 'Nemotron 3.5 Compact', size: '793 MB', langs: '35', tag: 'Fast responses' },
      { name: 'Whisper Base', size: '142 MB', langs: '99', tag: 'Smallest download' },
      { name: 'Whisper Small', size: '466 MB', langs: '99', tag: 'Balanced' },
      { name: 'Whisper Large-v3-Turbo', size: '1.5 GB', langs: '99', tag: 'Highest accuracy', highlight: true },
    ],
  },
  privacy: {
    eyebrow: 'Privacy',
    title: 'Your voice stays on your computer.',
    lead: 'No account to create, nothing to sign into, no audio sent anywhere. What you say is between you and your machine.',
    stats: [
      { value: '0', label: 'accounts required' },
      { value: '0', label: 'bytes of audio uploaded' },
      { value: 'Local', label: 'history, stored as 16 kHz WAV on disk' },
      { value: '1 click', label: 'to clear history and its audio' },
    ],
  },
  download: {
    title: 'Start talking to your keyboard.',
    lead: 'Grab the installer for your system. Models download inside the app.',
    platforms: {
      mac: { name: 'macOS', req: 'Apple Silicon (M1 or newer)', cta: 'Download for Mac' },
      windows: { name: 'Windows', req: '64-bit Intel / AMD', cta: 'Download for Windows' },
      linux: { name: 'Linux', req: '64-bit, X11 with a system tray · Wayland planned', cta: 'Download for Linux' },
    },
    allReleases: 'All releases and release notes',
  },
  faq: {
    title: 'Questions',
    items: [
      { q: 'Does my audio ever leave my computer?', a: 'No. Transcription runs locally. Plume only goes online to download speech models and to check GitHub Releases for updates.' },
      { q: 'Why does my system show a security warning?', a: "Installers aren't publisher-signed or notarized yet, so macOS and Windows may warn you on first launch. The full source is on GitHub if you'd rather build it yourself." },
      { q: 'Can I change the shortcuts?', a: 'Yes, in Settings → Dictation. By default: hold Ctrl + Space to talk, Ctrl + Shift + Space for hands-free, Esc to cancel.' },
      { q: 'Which model should I pick?', a: 'Whisper Small is a good start. Go Large-v3-Turbo for the best accuracy, Nemotron for speed, or Base if disk space is tight.' },
      { q: 'Does it work on Wayland?', a: 'Not yet. Linux builds currently need X11 and a system tray. Wayland support is planned.' },
    ],
  },
  footer: { made: 'Plume · Made in France', releases: 'Releases' },
  floating: { download: 'Download' },
};

export type Strings = typeof en;

const fr: Strings = {
  meta: {
    title: 'Plume — Parlez. Relâchez. Continuez d’écrire.',
    description:
      'Dictée push-to-talk pour macOS, Windows et Linux. Maintenez un raccourci, parlez, relâchez : Plume tape vos mots dans n’importe quelle app. Modèles locaux, sans compte, sans envoi audio.',
  },
  nav: {
    features: 'Fonctionnalités',
    models: 'Modèles',
    privacy: 'Confidentialité',
    faq: 'FAQ',
    download: 'Télécharger',
    theme: 'Basculer le mode sombre',
    menu: 'Navigation principale',
  },
  hero: {
    badge: '100 % sur votre ordinateur · macOS, Windows & Linux',
    title1: 'Parlez. Relâchez.',
    title2: 'Continuez d’écrire.',
    lead: 'Maintenez un raccourci, dites ce que vous pensez, relâchez. Plume tape vos mots dans l’app où vous êtes — en privé, sans compte et sans envoi audio.',
    download: 'Télécharger Plume',
    downloadFor: 'Télécharger pour',
    source: 'Voir le code sur GitHub',
    hold: 'Maintenez',
    toTalk: 'pour parler',
    windowTitle: 'Notes — Point produit',
    docTitle: 'Point produit',
    docLine1: 'On livre d’abord l’onboarding derrière un flag, puis on l’ouvre à tout le monde quand les textes sont finalisés.',
    docLine2:
      'Ajoutons aussi une courte vidéo sur la page de téléchargement pour montrer le raccourci avant l’installation',
  },
  pill: {
    ready: 'Prêt',
    listening: 'Écoute',
    transcribing: 'Transcription locale',
    inserted: 'Inséré',
  },
  steps: [
    { title: 'Maintenez le raccourci', text: 'Où que soit votre curseur — un message, un doc, un prompt. Plume vous écoute.' },
    { title: 'Parlez naturellement', text: 'Parlez comme vous pensez. Mains libres ? Ctrl + Maj + Espace pour démarrer et arrêter.' },
    { title: 'Relâchez. C’est écrit.', text: 'Le texte arrive là où vous écriviez. Vous changez d’avis en pleine phrase ? Échap annule.' },
  ],
  features: {
    eyebrow: 'Fonctionnalités',
    title1: 'Une petite app.',
    title2: 'Qui se fait oublier.',
    anyApp: {
      title: 'Fonctionne dans toutes vos apps',
      text: 'Messages, notes, documents, prompts IA. Insertion automatique, par collage ou par frappe — avec un repli presse-papiers optionnel.',
      chips: ['Slack', 'Mail', 'Notion', 'VS Code', 'Claude', 'Google Docs'],
      chipLast: 'tout champ de texte',
      demos: [
        { app: 'Slack', context: '# design', text: 'Top, on livre après le déjeuner.' },
        { app: 'Mail', context: 'Re : Planning T3', text: 'Merci Sam, jeudi me va très bien.' },
        { app: 'VS Code', context: 'retry.rs', text: '// Réessayer une fois avant d’afficher l’erreur' },
        { app: 'Claude', context: 'Nouvelle conversation', text: 'Résume ce fil en trois points.' },
      ],
    },
    items: [
      { icon: 'local', title: '100 % local', text: 'La transcription se fait sur votre machine. Internet ne sert qu’à télécharger les modèles et vérifier les mises à jour.' },
      { icon: '99', title: 'Langues', text: 'Les modèles Whisper comprennent 99 langues. Détection automatique, ou forcez le français ou l’anglais.' },
      { icon: 'history', title: 'Rien ne se perd', text: 'Historique local avec durée de conservation. Copiez, supprimez ou récupérez vos huit derniers enregistrements.' },
      { icon: 'theme', title: 'Dans votre barre de menus', text: 'Discret dans la barre de menus ou la zone de notification. Clair, sombre ou selon le système.' },
      { icon: 'bolt', title: 'Natif et rapide', text: 'Écrit en Rust avec GPUI. Pas d’Electron, pas d’onglet de navigateur caché en arrière-plan.' },
    ],
  },
  models: {
    eyebrow: 'Modèles',
    title: 'Choisissez votre compromis.',
    lead: 'Téléchargez un modèle une fois depuis l’app. Changez quand vous voulez. Ensuite, tout fonctionne hors ligne.',
    head: ['Modèle', 'Téléchargement', 'Langues', 'Idéal pour'],
    rows: [
      { name: 'Nemotron 3.5 Compact', size: '793 Mo', langs: '35', tag: 'Réponses rapides' },
      { name: 'Whisper Base', size: '142 Mo', langs: '99', tag: 'Le plus léger' },
      { name: 'Whisper Small', size: '466 Mo', langs: '99', tag: 'Équilibré' },
      { name: 'Whisper Large-v3-Turbo', size: '1,5 Go', langs: '99', tag: 'Précision maximale', highlight: true },
    ],
  },
  privacy: {
    eyebrow: 'Confidentialité',
    title: 'Votre voix reste sur votre ordinateur.',
    lead: 'Aucun compte à créer, aucune connexion, aucun audio envoyé. Ce que vous dites reste entre vous et votre machine.',
    stats: [
      { value: '0', label: 'compte requis' },
      { value: '0', label: 'octet d’audio envoyé' },
      { value: 'Local', label: 'historique stocké en WAV 16 kHz sur le disque' },
      { value: '1 clic', label: 'pour effacer l’historique et son audio' },
    ],
  },
  download: {
    title: 'Parlez à votre clavier.',
    lead: 'Récupérez l’installeur pour votre système. Les modèles se téléchargent dans l’app.',
    platforms: {
      mac: { name: 'macOS', req: 'Apple Silicon (M1 ou plus récent)', cta: 'Télécharger pour Mac' },
      windows: { name: 'Windows', req: '64 bits Intel / AMD', cta: 'Télécharger pour Windows' },
      linux: { name: 'Linux', req: '64 bits, X11 avec zone de notification · Wayland prévu', cta: 'Télécharger pour Linux' },
    },
    allReleases: 'Toutes les versions et notes de version',
  },
  faq: {
    title: 'Questions',
    items: [
      { q: 'Mon audio quitte-t-il mon ordinateur ?', a: 'Non. La transcription est locale. Plume ne se connecte que pour télécharger les modèles et vérifier les mises à jour sur GitHub Releases.' },
      { q: 'Pourquoi mon système affiche un avertissement de sécurité ?', a: 'Les installeurs ne sont pas encore signés ni notarisés, macOS et Windows peuvent donc avertir au premier lancement. Le code source complet est sur GitHub si vous préférez compiler vous-même.' },
      { q: 'Puis-je changer les raccourcis ?', a: 'Oui, dans Réglages → Dictée. Par défaut : maintenez Ctrl + Espace pour parler, Ctrl + Maj + Espace pour le mode mains libres, Échap pour annuler.' },
      { q: 'Quel modèle choisir ?', a: 'Whisper Small est un bon départ. Large-v3-Turbo pour la meilleure précision, Nemotron pour la vitesse, ou Base si l’espace disque est limité.' },
      { q: 'Est-ce que ça marche sous Wayland ?', a: 'Pas encore. Sous Linux, il faut pour l’instant X11 et une zone de notification. Le support Wayland est prévu.' },
    ],
  },
  footer: { made: 'Plume · Fait en France', releases: 'Versions' },
  floating: { download: 'Télécharger' },
};

export const strings: Record<Lang, Strings> = { en, fr };

export function t(lang: Lang): Strings {
  return strings[lang] ?? en;
}

export const KEYS: Record<Lang, { ctrl: string; space: string; esc: string }> = {
  en: { ctrl: 'Ctrl', space: 'Space', esc: 'Esc' },
  fr: { ctrl: 'Ctrl', space: 'Espace', esc: 'Échap' },
};
