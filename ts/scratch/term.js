const { Tabnas } = require('@tabnas/parser')
const { abnf } = require('../dist/abnf.js')
const run = (label, src, input) => {
  let out
  try { const tn = new Tabnas({ plugins: [abnf] }); tn.abnf(src); out = JSON.stringify(tn.parse(input)) }
  catch (e) { out = 'ERR ' + e.message.split('—')[0].trim().slice(0, 55) }
  console.log(label.padEnd(30), JSON.stringify(input), '->', out)
}
run('*DIGIT (terminal directly)', 'top = *DIGIT   ; @array', '123')
run('1*DIGIT', 'top = 1*DIGIT   ; @array', '123')
run('*item, item = "x" (lifted)', 'top = *item   ; @array\nitem = "x"', 'xxx')
run('*( "," )', 'top = *( "," )   ; @array', ',,')
run('control: *item, item=DIGIT', 'top = *item   ; @array\nitem = DIGIT', '123')
