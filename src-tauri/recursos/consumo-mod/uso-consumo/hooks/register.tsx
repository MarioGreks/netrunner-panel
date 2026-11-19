import type { Register } from 'claude-code'

// el perfil del usuario se pide al arrancar cada vez: así vale en cualquier equipo
async function rutaUso($: any): Promise<string | undefined> {
  const home = (await $.env.get('USERPROFILE')) ?? (await $.env.get('HOME'))
  return home ? `${String(home).split('\\').join('/')}/.claude/statusbar/usage.json` : undefined
}
const CLAVES: Record<string, 'fiveHour' | 'sevenDay'> = { five_hour: 'fiveHour', seven_day: 'sevenDay' }

export const register: Register = on => {
  // las ventanas que reporta Claude se vuelcan al fichero que lee Consumo-App (mismo formato que statusbar/statusline.js)
  on('session.measure', async ($, e, next) => {
    const nuevo: Record<string, { pct: number; resetsAt: number }> = {}
    for (const w of e.rateLimits) {
      const k = CLAVES[w.kind]
      const reset = w.resetsAt ? Math.floor(Date.parse(w.resetsAt) / 1000) : NaN
      if (k && !Number.isNaN(reset)) nuevo[k] = { pct: w.percentUsed, resetsAt: reset }
    }
    const USAGE = await rutaUso($)
    if (USAGE && Object.keys(nuevo).length > 0) {
      // una ventana que falta en esta lectura conserva su valor anterior
      let previo = {}
      try { previo = JSON.parse(await $.fs.read(USAGE)) } catch {}
      await $.fs.write(USAGE, JSON.stringify({ ...previo, ...nuevo, source: 'mod', ts: Math.floor(await $.clock.now() / 1000) }))
    }
    return next(e)
  }).catch(($, e, next) => next(e))
}
