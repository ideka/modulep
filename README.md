# Text Translator Module Protocol

Text Translator lets you create and load different Translator Modules to dynamically translate the game's text to different languages or using different methods.

**Without an attached Translator Module, Text Translator effectively does nothing.**

In theory, Translator Modules can be created in any language and using any technologies, but this `modulep` library is provided to help make modules in Rust.


## Module Protocol Overview

Communication between Text Translator ("the addon") and a Translator Module ("the module") happens through standard input, standard output, and standard error.

The module can send messages to the addon by writing to stdout, and must receive messages the addon sends by reading stdin. Additionally, the module may use stderr for logging purposes.

stdin and stdout **must** be treated as binary streams. Additionally, make sure you **flush** stdout after writing to it so it's actually sent.

**Warning**: Do **not** write random stuff to stdout, and do **not** use it for logging, as this will simply corrupt the message stream, requiring a module restart to fix.


## Message Format (stdin, stdout)

All integers are always in little endian, and are always unsigned unless specified otherwise.

Strings must always be valid UTF-8 and are length-prepended. String lengths are always two bytes only; this is because the game itself does not support strings longer than `0xFFFF` bytes. The string length is in bytes, not characters, and does *not* include the two length bytes themselves.

Messages are also always length-prepended, but use four bytes for length instead. Once again, the length does *not* include the four length bytes themselves.

Message size is capped at exactly `0x10'0000` bytes (not counting the four length bytes). Bigger messages will be rejected for safety, and to avoid allocating up to 4 GB of memory on potentially corrupted data.

All messages (with exception of the `Handshake`, see below) start with a `kind` byte that determines the type and format of the message. If your module can't recognize or handle a given message kind, it should ignore it by reading the rest of the message as per the message's prepended length, so it can then safely read the next message.

```c
struct Message {
  uint32_t length;
  struct Content content; // `sizeof(struct Content) == length`
};

struct Content {
  uint8_t kind;
  // ...
};

struct String {
  uint16_t length;
  uint8_t bytes[/* exactly `length` */];
};
```


### Handshake

The `Handshake` is a special type of message. It is length-prefixed like all the others, but it's the only one that doesn't have a `kind`. Instead, **it must be the very first thing the module reports** as soon as it's started.

Take care not to delay the delivery of the `Handshake`, as it can make the game freeze while waiting for it. In the future modules may be loaded in a separate thread to avoid this.

```c
struct Handshake {
  uint32_t protocol;
  int32_t source_lang;
  uint32_t result_version;
  struct String cache_key;
};
```

* `protocol` is the version of the Text Translator Module Protocol your module was built for.

  Currently this should be set to `1` or the addon will refuse to connect with the module. Future protocol updates may be backwards compatible (and thus modules that report an older version will still be loaded, but perhaps missing a few features), or in some cases may not (and then older modules will have to be updated to be usable again).

  The addon is planned to always refuse to load modules with a higher protocol version than itself, however this shouldn't be an issue as the addon should be easy to update.

* `source_lang` is the language your module translates **from**. Supported languages are:
  * English = 0
  * Korean = 1
  * French = 2
  * German = 3
  * Spanish = 4
  * Chinese = 5

  Keep in mind while technically those are all correct, Korean isn't actually provided in the game's files and therefore cannot be used to translate from, and while Chinese *is* in the game's files, it isn't normally selectable and thus is normally unusable as well.

  When your module is loaded, the user will be required to select this as their game's language, and your module will only receive text in this language for translation.

* `result_version` is your own internal translation version.

  All translated text is automatically cached by the addon so your module never has to translate the same thing twice. However if at some point you improve the translation method of your module, you may bump this value, thereby effectively marking all previous translations that have the old value as "stale". Stale translations are still shown, but the addon will also request your module translate them again. At this point, the stale translation is swapped with the new one on screen, and in the cache.

* `cache_key` exists to separate different translation types your module may be able to produce.

  For example, your module may translate to two separate languages. In cases like that, ensure you reply with a different `cache_key` for each language, or the two languages would get mixed up in the cache.

  The `cache_key` must be at most 32 bytes long. This will be written to cache with each piece of translated text, and there's no need to make it any longer. A two letter language code such as "jp" or "it" is often good enough.


### Addon Messages (stdin)

These are the messages the addon sends, which modules should be able to handle as soon as they write their `Handshake`.

* `Text` Message

  ```c
  struct Text {
    uint8_t kind; // always 0
    uint32_t id;
    struct String text;
  };
  ```

  A request to translate a piece of text. Has a `kind` of `0`, followed by the text ID in four bytes, followed by a string.

  The addon will send the module any and all pieces of text the game tries to show on screen or load. Ensure your module is always ready to read all messages and doesn't let the buffer fill up too much.

  The addon will not send any text in a different language than requested in the `Handshake`. The addon will also never send text that is currently waiting a response, and it won't send any text that has already been translated and cached (unless judged stale as per `result_version`), so do not bother deduping the requests.

  The game has some text entries that are empty as well, but the addon will never send those for translation.

  The addon will not batch these and instead send them as they come; however, these messages still tend to come in bursts, so it's recommended modules collect many at a time to translate in a batch.

Currently, this is the only message type the addon will send. In the future, other types may be added. Thus modules should make sure to ignore unknown messages properly by consuming them using their lengths as described earlier.


### Module Messages (stdout)

These are messages your module must write to stdout to communicate with the addon.

* `Text` Message

  ```c
  struct Text {
    uint8_t kind; // always 0
    uint32_t id;
    struct String text;
  };
  ```

  Same as the addon message of the same name, but this time around containing the translated text. Ideally you'd write a whole batch of these and then flush them all at once.

* `TextCancel` Message

  ```c
  struct TextCancel {
    uint8_t kind; // always 1
    uint32_t id;
  };
  ```

  Has a `kind` of `1` and just a four byte ID. You may use this message type if your module failed to translate a given piece of text. You may also use this message if your module is overwhelmed by requests, to cancel some of them.

  However keep in mind cancelled requests may be requested again. If you'd rather a given piece of text not be re-requested, you may simply ignore it and let the addon wait indefinitely for the response.


### Module Logging (stderr)

Your module may make use of stderr for logging. When connected to the addon, the addon will redirect such logs to Nexus's log. 

Due to the nature of the stderr stream, **messages are split by newlines**. This means a message with a `\n` in the middle will be interpreted as two separate messages. If you'd like to log a multiline message, you may use `\0` as a linefeed instead. `\x03` (ETX) may also be used for the same purpose in languages that use null-terminated strings.

Incoming messages are always categorized as `Info` unless prepended with a special letter for each log level.

* `e: Message` -> `Error`
* `w: Message` -> `Warning`
* `i: Message` -> `Info`
* `d: Message` -> `Debug`
* `t: Message` -> `Trace`

Note the syntax is `[letter][colon][space][message]`. The space is important.

E.g.:

```c
fprintf(stderr, "Test");
// Result:
// * "Info: Test"

fprintf(stderr, "t: Newline\ntest");
// Result:
// * "Trace: Newline"
// * "Info: test"

fprintf(stderr, "e: Error test");
// Result:
// * "Error: Error test"

fprintf(stderr, "w: Multiline\x03warning\x03message");
// Result:
// * "Warning: Multiline
// warning
// message"
```

The Rust module handles this for you and lets you use the `log` crate as normal. `\n` are also cheaply converted to `\0` before writing to stderr so it works as expected.


## Module Lifecycle

A module starts when either:

* The user starts it manually
* The addon first loads, and automatically starts its last used module; this can happen while the game is running or when the game first starts

And a module closes when:

* The user stops it manually
* The addon is unloaded
* The game is closed

The addon tries to ensure the module closes on unload, exit or even game crashes, but your module should also ensure it exits when stdin closes, and cleans up any spawned child processes. 


## Module Metadata

In order for the addon to recognize your module, a metadata file must be created.

Modules must be placed each in its own folder in: `[GW2]/addons/text_translator/modules/[folder]/`.

The folder name acts as an ID for your module, the addon will remember the name of the last started module so it can start it again automatically when the addon reloads.

Inside this folder must live a `module.toml` file with the following format:

```toml
name = "Name of your module"
command = "command to execute to run your module"
args = ["any args", "your module command needs"]
```

For example:

```toml
name = "My Module"
command = "my_module.exe"
args = []
```

Or:

```toml
name = "My Python Module"
command = "python.exe"
args = ["my_module.py"]
```

The `name` field will be shown in the addon's UI; try to keep it succinct. The command will be run using the module's folder as its working directory.

This folder will also contain your module's cache database (`cache.db`), created by the addon when your module is used. You may choose to prebuild this database with the addon and bundle it along with your module to give users a translation headstart. It's a sqlite database that uses a `WAL` journaling mode, so make sure you stop your module and the `db-shm` and `db-wal` files disappear before you copy it somewhere. A better tool for database prebuilding may be provided in the future.

Your module may also use this folder to store a config file or anything else it may need. Avoid using the `module.toml` file itself for configuration though.


## Format Considerations

Text strings in Guild Wars 2 come in many shapes and sizes, and your module must be able to handle all of them.

### Markdown

The game makes use html-like markdown tags for formatting in various places such as chat.

Some examples include...

* `<c=X></c>` tags to set a color, either in hex like `#f8c56e` or by reference like `@flavor`.

* `<quote=X>`

* `<a=X></a>`

Be warned **this is by no means an exhaustive list**.

These tags can easily break with improper translation; ensure your module leaves all tags untouched.


### Markers

The game also uses different types of formatting markers, which should be treated with care. Different source languages also have different markers that must be considered.

Here's a list of markers used in English, with made up examples:

* `%strX%` is a placeholder for a different string to be interpolated in later. `X` can be any one digit (except zero maybe).

* `%numX%` same as `%strX%` but for numbers.

* `[s]` is used as an optional pluralizing suffix. `%num1% Turnip[s]` can become both `1 Turnip` and `10 Turnips`.

* `[the]` is an optional `the`. `%str1% of [the] %str2%` can become both `Gloves of the Scholar` and `Gloves of Lyssa`.

* `[an]` could become either `an`, `a`, `a(n)`.

* `[topic-f]`/`[topic-m]`/`[f]`/`[m]` speculated to mark male/female variants of the same word but it's unknown how they work exactly.

* `[plur]` perhaps used to mark that the next `[s]` or `[pl:]` should use the plural version but it's unknown.

* `[lbracket]`/`[rbracket]` replaced with literal `[` and `]` respectively.

* `[null]` replaced with `\0`.

* `[nosep]` speculated it's used at the end of a string to mark that there should be no separation with the next string when two are concatenated together.

* `[f:"XXX"]` used to mark and specify the female version of a word. So `Fractal God[f:"Goddess"]` can become either `Fractal God` or `Fractal Goddess`.

* `[pl:"XXX"]` for cases when `[s]` is not enough. `%num1% Focus[pl:"Foci"]` -> `1 Focus` / `3 Foci`.

Once again **this is not an exhaustive list**.

German seems to have special markers for male/female/neutral genders and pluralization.

French seems to have markers for French articles like:
- `le`, `la`, `les`, `l'`, `un(e)`, `une`, `un`, `des`, `du`, `de l'`, `au`, `aux`

Spanish seems to have markers for Spanish articles like:
- `el`, `la`, `los`, `las`, `un`, `una`, `unos`, `unas`

**Failure to handle all markers correctly could well result in game crashes.**


### Trivial Strings

Many strings in the game are so trivial as to not require any translation at all. Your module should recognize these and return them as-is to avoid wasting time or resources.

* A string may only contain markers such as `[null]`.

* A string may simply say `(new string)`.

* A string may be a double parenthesized number such as `((12345))`.

* A string may only contain `%numX%` and/or `%strX%` templates.
