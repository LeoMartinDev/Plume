# Plume translations

Interface text lives in this directory, with one UTF-8 JSON file per language.
`en.json` is the English reference catalog; `fr.json` contains French translations.

## Edit a translation

Open the language file and edit the value next to the message key:

```json
{
  "name": "Français",
  "messages": {
    "common.next": "Suivant",
    "language.interface": "Langue de l’interface"
  }
}
```

Keep the keys unchanged: they identify messages in the application. Apostrophes
and accented characters can be entered directly. Escape double quotes inside
values (`\"`). JSON does not allow comments or trailing commas.

## Add an interface language

1. Copy `en.json` to a new file, such as `es.json` or `pt-br.json`.
   Use a lowercase language code as the filename.
2. Set `name` to the language's native name, such as `Español`.
3. Translate the values in `messages`, keeping the keys unchanged.
4. Rebuild the app: `cargo build -p plume-app --bin plume --locked`.
   The new language appears automatically in the onboarding and settings
   selectors. No Rust files need to be changed.

Translations can be incomplete: missing keys and empty values fall back to
English. Catalogs are embedded during compilation; editing a JSON file does not
change an executable that has already been built.

Preserve placeholders such as `{count}` exactly. You can move them within the
sentence, but do not rename or remove them. For example, `"{count}m ago"` becomes
`"il y a {count} min"` in French.

## Add a new application message

Add a descriptive, stable key to `en.json`, such as
`onboarding.languages.title`, then add translations under the same key.
Application code displays the message with `tr("onboarding.languages.title")`.
Dictated text and published release notes are not translated by these catalogs.

## Validate the catalogs

```sh
cargo test -p plume-app --lib --locked
```

The build validates JSON syntax, filenames, display names, and value types.
Tests check message keys and translation placeholders.

To preview a language without downloading a model:

```sh
cargo run -p plume-app --example onboarding_preview -- languages --interface-language=fr
```

Interface language is configured under **Interface**, alongside the theme.
Dictation language is configured under **Models**. Adding an interface language
here does not add speech recognition support: dictation languages depend on the
engine and model compatibility.
