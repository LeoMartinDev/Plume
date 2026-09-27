# Model settings — focused UX review

## Scope

- Surface: Model settings page.
- User goal: understand the installed speech model and choose its recognition language.
- Evidence: `01-model-settings-grouped.jpg`.

## Step 1 — Model settings

Health: good.

- Strengths: compact navigation, clear installed state, short labels, native control sizing, and no decorative copy.
- Original issue: the model and language rows floated independently in a large white surface, which made the page feel unfinished.
- Resolution: one inset 1 px hairline now connects the two rows as a single settings group. No shadow, extra card, colored background, or additional description was introduced.
- Accessibility risk: the separator is decorative and does not carry meaning by itself. Keyboard focus, screen-reader labels, and contrast under every Windows theme still require behavioral testing beyond this screenshot.

## Recommendation

Keep this single separator. Adding more dividers, section captions, or illustrations would reduce the minimal native feel without improving the task.
