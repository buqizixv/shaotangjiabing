const fs = require('fs');
const path = require('path');
function integrateAI(source) {
  const controller = fs.readFileSync(path.join(__dirname, 'ai-controller.splash'), 'utf8');
  source = source.replace(/\/\/ BEGIN ONWAY AI[\s\S]*?\/\/ END ONWAY AI\n/g, '');
  source = source.replace(/\/\/ BEGIN ONWAY AI UI[\s\S]*?\/\/ END ONWAY AI UI\n/g, '');
  source = source.replace(/\/\/ BEGIN ONWAY AI ENTRY[\s\S]*?\/\/ END ONWAY AI ENTRY\n/g, '');
  const style = 'let OnwayAI = TextStyle{font_family: FontFamily{latin := FontMember{res: http_resource("{{assets}}/assets/NotoSansSC-AI-Regular.otf") asc: -0.2 desc: 0.08 weight: 400}} line_spacing: 1.2}\n';
  source = source.replace('let OnwayRegular =', '// BEGIN ONWAY AI\n' + controller + '\n' + style + '// END ONWAY AI\nlet OnwayRegular =');
  for (const marker of ['// BEGIN ONWAY AI\n']) {
    if (!source.includes(marker)) throw Error('AI integration point missing: ' + marker);
  }
  return source;
}
module.exports = integrateAI;
if (require.main === module) {
  const target = path.join(__dirname, 'bundle/main.splash');
  fs.writeFileSync(target, integrateAI(fs.readFileSync(target, 'utf8')));
}
