# @medousa/liquid-markdown

Dependency-free Liquid Markdown grammar, payload types, and inert placeholder
encoding shared by Medousa's first-party rendering surfaces.

The package intentionally does not own a Markdown engine, UI framework, host
navigation, image resolution, or network access. Browser rendering is exposed
separately so hosts can supply those capabilities.
