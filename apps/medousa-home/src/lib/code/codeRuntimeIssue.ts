/** Presentation only. Recovery authority remains with the workshop. */
export function codeRuntimeIssue(message: string): { summary: string; guidance: string; details: string } {
  const details = message.trim();
  let cause = details;
  const jsonStart = details.indexOf('{');
  if (jsonStart >= 0) {
    try {
      const payload = JSON.parse(details.slice(jsonStart));
      if (typeof payload.error === 'string') cause = payload.error;
    } catch { /* Non-JSON runtime errors still receive a readable summary. */ }
  }
  const branch = /attached checkout switched branches: expected (.+), found (.+)/i.exec(cause);
  if (branch) return {
    summary: 'The working copy changed branches.',
    guidance: `This project was attached to ${branch[1]}. The working copy is now on ${branch[2]}. Return to the expected branch on the workshop, or release this project and attach the current checkout again. Keep your drafts before releasing it.`,
    details,
  };
  if (/environment drifted|attached checkout (HEAD|index|root|now|entered)/i.test(cause)) return {
    summary: 'The working copy changed outside this project.',
    guidance: 'Review the working copy on the workshop before continuing. Existing drafts are still here; Medousa will not switch branches or move an editing session automatically.', details,
  };
  if (/lease|custody|owned by|another executor|active attempt/i.test(cause)) return {
    summary: 'Editing control is required.',
    guidance: 'Use Resume editing to request control, then try again. Existing processes remain with their original working copy.', details,
  };
  if (/HTTP\s+(404|405)|unsupported|requires a newer|not support/i.test(cause)) return {
    summary: 'This workshop does not support this action.',
    guidance: 'Update the workshop software, then retry this action.', details,
  };
  if (/HTTP\s+|\{"|network|fetch failed|connection refused/i.test(cause)) return {
    summary: 'The workshop could not complete this action.',
    guidance: 'Check the workshop connection and try again. Your drafts are preserved.', details,
  };
  return { summary: cause, guidance: '', details };
}
