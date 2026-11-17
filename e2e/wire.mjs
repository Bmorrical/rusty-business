// The smallest Redis and mail clients that will do, so this suite installs
// nothing. Each one opens a connection, says its piece, and closes.

import net from 'node:net';

const REPLY_LIMIT_MS = 10_000;

function connect(host, port) {
  return new Promise((resolve, reject) => {
    const socket = net.createConnection({ host, port });
    socket.setTimeout(REPLY_LIMIT_MS, () => {
      socket.destroy(new Error(`${host}:${port} went quiet`));
    });
    socket.once('connect', () => resolve(socket));
    socket.once('error', reject);
  });
}

// Hands back the bytes a connection sends, one await at a time.
function reader(socket) {
  let held = Buffer.alloc(0);
  let wake = null;
  let failure = null;
  const nudge = () => {
    if (wake) {
      wake();
      wake = null;
    }
  };
  socket.on('data', (chunk) => {
    held = Buffer.concat([held, chunk]);
    nudge();
  });
  socket.on('error', (reason) => {
    failure = reason;
    nudge();
  });
  socket.on('close', () => {
    failure ??= new Error('the connection closed early');
    nudge();
  });
  return {
    // Calls take(bytes) until it returns { value, used } rather than null.
    async next(take) {
      for (;;) {
        const taken = take(held);
        if (taken) {
          held = held.subarray(taken.used);
          return taken.value;
        }
        if (failure) throw failure;
        await new Promise((resolve) => {
          wake = resolve;
        });
      }
    },
  };
}

// ---- Redis ---------------------------------------------------------------

function encodeCommand(words) {
  let out = `*${words.length}\r\n`;
  for (const word of words) {
    const text = String(word);
    out += `$${Buffer.byteLength(text)}\r\n${text}\r\n`;
  }
  return out;
}

// Reads one Redis reply starting at `at`. Null means "not all here yet".
function readReply(bytes, at = 0) {
  const lineEnd = bytes.indexOf('\r\n', at);
  if (lineEnd === -1) return null;
  const kind = String.fromCharCode(bytes[at]);
  const line = bytes.toString('utf8', at + 1, lineEnd);
  const afterLine = lineEnd + 2;
  if (kind === '+') return { value: line, used: afterLine };
  if (kind === ':') return { value: Number(line), used: afterLine };
  if (kind === '-') return { value: new Error(`Redis said: ${line}`), used: afterLine };
  if (kind === '$') {
    const length = Number(line);
    if (length < 0) return { value: null, used: afterLine };
    if (bytes.length < afterLine + length + 2) return null;
    return {
      value: bytes.toString('utf8', afterLine, afterLine + length),
      used: afterLine + length + 2,
    };
  }
  if (kind === '*') {
    const count = Number(line);
    if (count < 0) return { value: null, used: afterLine };
    const items = [];
    let cursor = afterLine;
    for (let i = 0; i < count; i += 1) {
      const item = readReply(bytes, cursor);
      if (!item) return null;
      items.push(item.value);
      cursor = item.used;
    }
    return { value: items, used: cursor };
  }
  throw new Error(`a Redis reply of a kind this client does not read: ${kind}`);
}

/**
 * Runs commands against Redis, in order, and returns their replies.
 *
 *   const [length] = await redis(url, ['LLEN', 'rusty:jobs']);
 */
export async function redis(url, ...commands) {
  const address = new URL(url);
  const socket = await connect(address.hostname, Number(address.port || 6379));
  const replies = reader(socket);
  const ask = async (words) => {
    socket.write(encodeCommand(words));
    const reply = await replies.next((bytes) => readReply(bytes));
    if (reply instanceof Error) throw reply;
    return reply;
  };
  try {
    if (address.password) {
      await ask(['AUTH', decodeURIComponent(address.password)]);
    }
    const answers = [];
    for (const words of commands) {
      answers.push(await ask(words));
    }
    return answers;
  } finally {
    socket.destroy();
  }
}

// ---- Mail ----------------------------------------------------------------

// One reply from a mail server: every line up to the one whose code is
// followed by a space.
function readMailReply(bytes) {
  let from = 0;
  for (;;) {
    const lineEnd = bytes.indexOf('\r\n', from);
    if (lineEnd === -1) return null;
    const line = bytes.toString('utf8', from, lineEnd);
    if (line[3] !== '-') {
      return { value: Number(line.slice(0, 3)), used: lineEnd + 2 };
    }
    from = lineEnd + 2;
  }
}

/** Sends one plain-text email to the mail server on this computer. */
export async function sendMail({ port, from, to, subject, body }) {
  const socket = await connect('127.0.0.1', Number(port));
  const replies = reader(socket);
  const expect = async (code, sent) => {
    const got = await replies.next(readMailReply);
    if (got !== code) {
      throw new Error(`after ${sent}, the mail server answered ${got}, not ${code}`);
    }
  };
  const say = async (line, code) => {
    socket.write(`${line}\r\n`);
    await expect(code, line.split(' ')[0]);
  };
  try {
    await expect(220, 'connecting');
    await say('HELO e2e.localhost', 250);
    await say(`MAIL FROM:<${from}>`, 250);
    await say(`RCPT TO:<${to}>`, 250);
    await say('DATA', 354);
    await say(
      [`From: ${from}`, `To: ${to}`, `Subject: ${subject}`, '', body, '.'].join('\r\n'),
      250,
    );
    await say('QUIT', 221);
  } finally {
    socket.destroy();
  }
}
