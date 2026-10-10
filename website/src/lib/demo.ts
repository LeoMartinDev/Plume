// Helpers for the looping product demos (hero window, "works in every app" card).
// Demos only run when motion is allowed, and only while mostly on screen in a visible tab:
// starting the moment the first pixel scrolls in would change the demo as people arrive on it.

export function motionAllowed(): boolean {
  return !window.matchMedia('(prefers-reduced-motion: reduce)').matches;
}

export interface Timeline {
  /** Resolves after `ms` of on-screen time (waits for the element to be in view first). */
  wait(ms: number): Promise<void>;
}

export function timeline(el: Element): Timeline {
  let intersecting = false;
  let resume: (() => void) | null = null;
  const visible = () => intersecting && !document.hidden;
  const check = () => {
    if (visible() && resume) {
      resume();
      resume = null;
    }
  };
  // Half the element, or most of the viewport when the element is taller than that
  const ratio = Math.min(0.5, (0.8 * window.innerHeight) / el.getBoundingClientRect().height);
  new IntersectionObserver(
    ([entry]) => {
      intersecting = entry.isIntersecting && entry.intersectionRatio >= ratio - 0.01;
      check();
    },
    { threshold: [0, ratio] },
  ).observe(el);
  document.addEventListener('visibilitychange', check);

  const whenVisible = () => (visible() ? Promise.resolve() : new Promise<void>((r) => (resume = r)));
  return {
    async wait(ms) {
      await whenVisible();
      await new Promise((r) => setTimeout(r, ms));
    },
  };
}

/**
 * Types `text` word by word. The untyped remainder stays in the layout as transparent
 * text, so lines wrap where they will end up and nothing jumps while typing.
 */
export async function typeWords(typed: HTMLElement, rest: HTMLElement, text: string, tl: Timeline, perWord = 60) {
  const words = text.match(/\S+\s*/g) ?? [];
  typed.textContent = '';
  rest.textContent = text;
  for (let i = 1; i <= words.length; i++) {
    await tl.wait(perWord + Math.random() * perWord * 0.6);
    typed.textContent = words.slice(0, i).join('');
    rest.textContent = words.slice(i).join('');
  }
}

/** Empties the line but keeps `text` in the layout (transparent), so its height never changes. */
export function resetText(typed: HTMLElement, rest: HTMLElement, text: string) {
  typed.textContent = '';
  rest.textContent = text;
}
