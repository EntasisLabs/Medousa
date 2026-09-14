# Draw and generate images in chat

Medousa treats drawings and generated images as real chat media. Their bytes
live with the active workshop daemon, so the same chat works when your phone or
tablet is connected to a remote workshop.

## Draw in the composer

Open the composer **+** menu and choose **Draw**. The drawing sheet uses the
same pressure brushes, eraser, selection, move, undo, and zoom behavior as a
drawing note. Choose **Add** when the sketch is ready.

Medousa sends two linked representations to the workshop:

- editable vector ink, used when you reopen the drawing; and
- a PNG preview, used in the transcript and by a vision-capable model.

Tap a drawing in chat to view it full screen. From there you can edit and add a
new revision to the composer, save the editable drawing as a vault note, copy,
share where the device supports it, or download a PNG. The original message is
never silently replaced.

## Generate an image

1. Open **Settings → Models**.
2. Set **Image generation** to **OpenAI → GPT Image 2**.
3. Add an OpenAI API key under **Providers** if one is not already configured.
4. Ask naturally in chat, for example: “Generate a wide watercolor illustration
   of a greenhouse at dusk.”

Medousa exposes image generation to the conversation model as a typed tool. It
shows the tool's progress in the turn, stores successful outputs under the
workshop's session authority, and then renders them inline. Portrait, square,
landscape, quality, background, and bounded multi-image requests are supported.

Open a generated image to refine it, save a durable copy into the vault, copy,
share, or download it. **Refine** attaches the prior output to the composer;
Medousa carries its generation lineage and the daemon uses the Image API edit
path when the model supplies that reference to the image tool.

## OpenAI sign-in and billing

OpenAI's two credential lanes are separate:

- **ChatGPT sign-in** powers Medousa's `openai-codex` conversation models.
- **OpenAI API key** powers the programmatic Image API and is billed as API
  usage.

Codex itself can generate images through its integrated image-generation
capability, but Medousa's current `openai-codex` backend contract does not expose
that capability for third-party invocation. Medousa therefore offers the API-key
Image API route today and returns an explicit unsupported result if an older
configuration names the account-backed route. It never forwards a ChatGPT token
to the Platform Image API.

## Storage and deletion

Chat media is session-scoped and deleted with the owning session. **Save to
vault** creates a self-contained vault note so the saved image survives session
deletion. Generated-image history retains provider/model and generation lineage,
but raw provider responses and credentials are not written into the transcript.
