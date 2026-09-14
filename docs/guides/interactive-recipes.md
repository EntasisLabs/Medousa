# Interactive recipes and procedures

When an answer is easier to follow step by step, Medousa can present it as an
interactive recipe or procedure. You do not need to ask for “Liquid” or name a
special format: ask naturally for a recipe, a timed process, or a guided set of
instructions.

An interactive recipe can include a title, servings or yield, ingredients or
other resources, ordered instructions, notes, and timers attached to individual
steps. Ordinary explanatory questions still use normal prose.

## Timers

Select **Start** on a timed step. You can pause, resume, or reset it without
sending another message to Medousa. Timers use an absolute deadline, so the
remaining time corrects itself after the app is suspended or reopened.

The first time you start a timer, Medousa may ask for notification permission.
If allowed, the device schedules one local notification for that step. Denying
permission does not disable the timer; it continues to reconcile from its saved
deadline when the app is open again.

Timer state belongs to the workshop daemon and is stored separately for each
conversation message and step. This keeps two timers with the same label from
colliding, including when Home is connected to a remote workshop.

## Follow-up actions

Buttons such as **Scale to 2 servings**, **Suggest a substitution**, or **Make a
shopping list** intentionally start a new conversational turn. Timer controls do
not. This distinction keeps local interaction fast and makes it clear when an
action asks Medousa to reason again.

## Other clients and exports

VS Code, Obsidian, and exported notes retain the complete ingredient/resource
list, ordered steps, notes, and duration labels. Those surfaces may show a static
version rather than live timer controls, but the procedure remains readable.
