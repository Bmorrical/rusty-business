// A stand-in rig, for trying the Rig pane with no cards: one small server
// per port, on this computer only, each answering the way llama-server does,
// a word at a time, so models asked at once can be seen answering at once.
//
//   node scripts/fake-rig.mjs           ports 8091 to 8094
//   node scripts/fake-rig.mjs 8091 8092
//
// Each server lists one model, phi-4, and answers any chat with which port
// it is. It loads nothing and reaches nothing. Stop it with Ctrl-C.

import http from 'node:http';

const HOST = '127.0.0.1';
const ports = process.argv.slice(2).map(Number);
const listening = ports.length ? ports : [8091, 8092, 8093, 8094];

/** One word of the answer, as a server-sent event. */
function chunk(text) {
  return `data: ${JSON.stringify({ choices: [{ delta: { content: text } }] })}\n\n`;
}

export function serve(port, host = HOST, pause = 150) {
  const server = http.createServer((request, response) => {
    if (request.method === 'GET' && request.url === '/v1/models') {
      response.writeHead(200, { 'Content-Type': 'application/json' });
      response.end(JSON.stringify({ object: 'list', data: [{ id: 'phi-4', object: 'model' }] }));
      return;
    }
    if (request.method !== 'POST' || request.url !== '/v1/chat/completions') {
      response.writeHead(404).end();
      return;
    }
    request.resume();
    request.on('end', () => {
      response.writeHead(200, { 'Content-Type': 'text/event-stream' });
      const words = `This is the fake rig on port ${port}, answering beside the others.`.split(' ');
      let next = 0;
      const timer = setInterval(() => {
        if (next < words.length) {
          response.write(chunk(`${next ? ' ' : ''}${words[next]}`));
          next += 1;
          return;
        }
        clearInterval(timer);
        response.end('data: [DONE]\n\n');
      }, pause);
      response.on('close', () => clearInterval(timer));
    });
  });
  return new Promise((resolve) => server.listen(port, host, () => resolve(server)));
}

if (import.meta.url === `file://${process.argv[1]}`) {
  for (const port of listening) {
    await serve(port);
    console.log(`fake rig model at http://${HOST}:${port}/v1`);
  }
}
