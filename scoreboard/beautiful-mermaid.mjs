// Draws the flowchart on stdin with beautiful-mermaid; the scoreboard runs it from the folder install.sh fills.
// Exit 2: the library threw a plain Error, a refusal. Exit 3: any other throw, a crash.
import { renderMermaidASCII } from 'beautiful-mermaid'

const chunks = []
for await (const chunk of process.stdin) chunks.push(chunk)
try {
  process.stdout.write(renderMermaidASCII(Buffer.concat(chunks).toString('utf8'), { colorMode: 'none' }))
} catch (error) {
  process.stderr.write(`${error?.message ?? error}\n`)
  process.exitCode = error?.constructor === Error ? 2 : 3
}
