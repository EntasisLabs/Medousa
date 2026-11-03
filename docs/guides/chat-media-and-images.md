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

## Refer back to a picture

In a later message, you can refer to an image already attached to this chat;
you normally do not need to upload it again. Medousa can include images from the
latest 20 transcript turns, with a limit of five images across the recent
history and the current request. PNG, JPEG, WebP, and GIF uploads use the same
image representation on the first turn and when reopened. Large images are
scaled to fit the model's input budget; the original upload is retained.
To revisit an older picture, ask Medousa to
reopen it from this chat. It can only use an image that is still available in
this session's workshop media. Deleted or unavailable images may be skipped.

Image understanding requires a vision-capable provider and model. A text-only
route cannot inspect the picture; switch to a vision-capable model when you
want an image analyzed.

## Inspect an image created through the terminal

Medousa can inspect image files without you attaching them to a message. For
example, ask: “Run the screenshot command, then inspect `screenshots/result.png`
and tell me whether the layout is clipped.” It can create the file with its
terminal tools, then open it with `cognition_image_view`.

The file must be on the active **workshop daemon's disk**, inside an authorized
coding folder. In Coder, this defaults to the admitted checkout. In an Assistant
turn, the agent supplies an existing authorized coding root; the default is the
scripts library. Save screenshots inside that folder rather than `/tmp`.
A file on your phone or a remote UI device still needs to be attached or moved
through an authorized transfer path first.

PNG, JPEG, WebP, and GIF (first frame) are supported, up to 25 MiB. Large images
are scaled for model input; the file itself is unchanged. The tool snapshots the
image at the time it opens it, so a later overwrite does not change what the
model saw. Image understanding still requires a vision-capable model.

A successful file receipt or `vision_available: true` means the file was read
and the selected route advertises vision; it is not a visual approval. Medousa
must also attach verified image bytes to the next model request. Vision-capable
workshop workers use this same hydration path. On the ChatGPT sign-in Responses
route, those pixels are sent as image content in the originating tool's output,
not as an extra user message. Other vision providers retain image-bearing model
messages. If pixels are unavailable, the agent should report that limitation
rather than infer a layout from filenames or metadata. On externally supplied
tool surfaces, it should call `cognition_image_view` directly: generic parallel
wrappers can return the receipt without forwarding the image attachment. If
that happens, a direct retry is needed before visual approval.

This is currently a daemon-local file tool. Bound container/remote coding
work-environment paths are refused explicitly, rather than read from the host.

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
