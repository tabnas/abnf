# @tabnas/abnf

ABNF grammar compiler for the [`tabnas`](https://github.com/rjrodger/tabnas) parser.

Takes ABNF source and emits a tabnas `GrammarSpec`. Also ships a
CLI (`tabnas-abnf`) that does the same thing from the shell.

## Install

```bash
npm install @tabnas/parser @tabnas/abnf
```

## Use

```js
const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('@tabnas/abnf')

const tn = new Tabnas({ plugins: [abnf] })
tn.abnf(`greet = "hi" / "hello"`)
tn.parse('hi').rule    // => 'greet'
tn.parse('hello').rule // => 'greet'
```

Or convert without installing:

```js
const { abnfConvert } = require('@tabnas/abnf')
const spec = abnfConvert(`greet = "hi"`)
spec.options.rule.start // => '__start__'
```

## CLI

```bash
tabnas-abnf -f grammar.abnf
tabnas-abnf 'g = "a"' --parse 'a'
```

## License

MIT.
