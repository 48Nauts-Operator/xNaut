// Snippet read-view structure. Fences are atomic commands; headings inside a
// fence are code, never section boundaries. No HTML from the snippet is run.
(function () {
  'use strict';
  function parse(source) {
    const lines = String(source || '').replace(/\r\n?/g, '\n').split('\n');
    const root = { children: [] };
    const stack = [{ level: 0, node: root }];
    const commands = [];
    let text = [], headings = 0, sectionId = 0;
    const append = node => stack[stack.length - 1].node.children.push(node);
    const flush = () => {
      if (text.some(line => line.trim())) append({ type: 'text', text: text.join('\n').trim() });
      text = [];
    };
    for (let i = 0; i < lines.length; i++) {
      const fence = lines[i].match(/^ {0,3}(`{3,}|~{3,})(.*)$/);
      if (fence) {
        flush();
        const closing = new RegExp(`^ {0,3}${fence[1][0]}{${fence[1].length},}[ \\t]*$`);
        const code = [];
        while (++i < lines.length && !closing.test(lines[i])) code.push(lines[i]);
        const command = code.join('\n');
        if (command.trim()) { commands.push(command); append({ type: 'code', text: command }); }
        continue;
      }
      const heading = lines[i].match(/^ {0,3}(#{1,6})(?:[ \t]+|$)(.*)$/);
      const underline = lines[i].trim() && lines[i + 1]?.match(/^ {0,3}(=+|-+)[ \t]*$/);
      if (heading || underline) {
        flush();
        const level = heading ? heading[1].length : underline[1][0] === '=' ? 1 : 2;
        const title = heading ? heading[2].replace(/[ \t]+#+[ \t]*$/, '').trim() : lines[i].trim();
        if (underline && !heading) i++;
        while (stack[stack.length - 1].level >= level) stack.pop();
        const node = { type: 'section', level, title, id: sectionId++, children: [] };
        append(node); stack.push({ level, node }); headings++;
      } else text.push(lines[i]);
    }
    flush();
    // Plain, unfenced snippets retain their existing one-command-per-line use.
    if (!commands.length) {
      const plain = lines.map(line => line.trim()).filter(line => line && !line.startsWith('#') && !line.startsWith('//'));
      const unfenced = blocks => blocks.flatMap(block => block.type === 'section' ? unfenced(block.children) : block.text.split('\n').map(line => line.trim()).filter(line => line && !line.startsWith('//')));
      return { blocks: headings ? root.children : plain.map(text => ({ type: 'code', text })), commands: headings ? unfenced(root.children) : plain };
    }
    return { blocks: root.children, commands };
  }
  window.xnautSnippetSections = { parse };
})();
