<!-- SPDX-License-Identifier: Apache-2.0 -->
# Designer text audit

Date: 2026-11-11. Scope: every family of visible prototype text, including the
initial canvas, task panels, selected/empty states, fixtures, dialogs and feedback.
This is a copy/interaction review, not a review of every paragraph of design prose.

## Decision rule

Keep text when it identifies an object/action, disambiguates scope, labels a value,
explains a consequence, or enables recovery. Move occasional explanations to Help
or source details. Remove narration of obvious controls, repeated context, empty
status filler and internal implementation terminology. Shorter is not automatically
better: variable names, units, scope and destructive consequences are necessary.
Do not replace meaningful visible labels with unexplained icons or hover-only help.

## Chrome and canvas

| Text | Decision | Reason / placement |
| --- | --- | --- |
| OTriOS / DESIGNER | Keep once | Tool identity |
| Component name / LabelledSlider | Keep | Identifies the edited component; Name remains an editable field |
| All changes applied / Unapplied changes | Keep one surface | Tells author whether Apply is needed |
| Invalid/review/pending deletion status | Keep | Calls attention to a required next step |
| New component, Undo, Redo, Discard, Apply, Publish… | Keep | Commands; context/confirmation establishes scope |
| Workspace | Remove visible group label | Library/Layers/Inspect tabs already identify navigation; retain accessible group name |
| Library, Layers, Inspect | Keep | Destination names and selected-tab identity |
| Interaction / Edit / Test | Keep | Names canvas input behavior; selected mode is persistent |
| Preview appearance / Dark / Light | Keep | Disambiguates local preview context from authored colors |
| Confirmed model; generation/base counters | Keep out of normal UI | Redundant/internal; counters remain in Prototype details |
| Edit selects layers; Test interacts… | Move to Help | Useful to learn once; not needed below every preview |
| Title · Text; Slider · Slider canvas badges | Remove visible badges | Repeat Layers and obscure content; retain accessible selection buttons/outline |
| Volume, 40 and other actual component content | Keep | These are the specimen being edited/tested, not Designer instructions |
| Instance A · local text override demonstration | Collapse as Override example | A qualification aid, not the main editing task |
| Set/reset override actions | Keep inside example | Renamed Override color / Reset to match the actual property |
| Source: prototype text color pair / Dark | Shorten | Inherited color / Local override conveys provenance without internal jargon |
| Routine Sample X = Y notices while dragging | Remove | The specimen already shows the value; avoid a second live narration stream |
| Ready. Open Library… | Remove | Unnecessary default footer instruction; footer is absent until needed |
| Routine Inserted/Applied/Discarded model prose | Suppress | Canvas, Apply status and history already show the outcome |
| Publication result | Keep on demand | Separate rare operation; prototype explicitly says simulated/no external change |
| Help | Keep collapsed entry | Discoverable explanation, independent of hover |
| Prototype details | Keep collapsed entry | RAM-only limits, missing capabilities and simulations are honest but optional technical context |

## Library and Layers

| Text | Decision | Reason / placement |
| --- | --- | --- |
| Component library | Shorten to Components | Active Library tab supplies context |
| Definition names, Retired marker | Keep | Object identity and availability |
| Insert control | Keep | Labels the control picker |
| Insert into root | Shorten to Add control | The prototype inserts into the current component; nested target UI is not implemented |
| Built-ins are references… | Move to Help | Important ownership model, occasional learning need |
| Layers heading | Keep | Identifies hierarchy region independently of accessible tab label |
| Layer name / type repeated as Slider / Slider | Show name once | Type appears in selected inspector; names remain stable authoring labels |
| Move up / Move down / Remove selected node | Keep | Clear non-drag commands and removal scope |
| No components / Add first control | Keep only in true empty state | Supplies the next action where the UI otherwise has no content |

## Inspector

| Text | Decision | Reason / placement |
| --- | --- | --- |
| Component; Name | Keep | Identity editing |
| Select a layer to edit or expose… | Remove | Persistent no-selection tutorial; relevant fields appear on selection |
| Selected type + internal node ID | Keep type only | Type distinguishes behavior; ID is technical metadata |
| Layer name; Content / caption | Keep distinct | Renaming must not change visible content |
| Layout / root | Rename Component layout | Makes root-only scope explicit in ordinary language |
| Flow; Horizontal/Vertical; Gap | Keep | Layout decisions and values |
| Width; Height; Fixed/Hug/Fill; Fixed W/H | Keep | Independent axes/modes; disabled Fixed values retain their meaning |
| Measured W … × H … · preview parent inner H 320 | Shorten to Actual W × H px | Useful resolved size; parent-fixture detail moves to Prototype details |
| Dimension bounds | Rename Size limits | Familiar description of Min/Max W/H |
| Min W/H; Max W/H; None | Keep | Bounds and absent-upper-bound meaning |
| Padding · all sides | Shorten to Padding | There is no individual-side toggle yet; uniform behavior belongs in Help |
| Distribution; Packed/Space between; Alignment | Keep | Distinct layout concepts with actual choices |
| Clip content | Keep | Changes overflow behavior |
| Style / panel.default | Rename Colors | No real named-style resolver exists in this prototype; avoid claiming a binding |
| Prototype color pairs · full resolver planned | Move to Prototype details | Capability limitation, not a property label |
| surface_background; text_primary | Keep | Descriptive color names are an explicit design choice |
| Expose selected value | Rename Component API | Section owns exposed properties; action button says Expose property |
| Property name; Type; Expose property | Keep when a node is selected | Needed to create a typed endpoint |
| Exposed: none | Remove | Empty result filler; hide an empty API section |
| Existing exposed property names | Keep even without node selection | Useful root API information must not disappear with the creation form |
| Definition lifecycle | Rename Component actions | User-facing action scope |
| N confirmed references in this model… | Shorten to N uses in this document | Keep dependency scope/count; detailed scan limitations belong in technical reference |
| Retire…; Delete… | Keep | Different effects; confirmation explains consequences |
| Validation/conflict text | Keep, concise and specific | Must explain what failed and how to recover; never replace with generic Error |

## Dialogs, sample controls and unsupported viewports

Keep object names, operation verbs, Cancel, dependency lists, retirement effects,
removal scope, and the fact that publication is simulated. Shorten internal
phrasing such as “confirmed references in this model” to document-scoped uses.
Do not conceal destructive consequences to reduce word count.

Keep sample button captions, Low/Medium/High choices, numeric readouts, tab/content
labels, list item names and Ready/Complete status within the specimens. They test
real content/layout states. Fixture actions such as Advance sample remain clearly
labeled as samples; they are not proposed production Progress APIs.

Keep the unsupported-viewport heading and one instruction: “Open Designer in a
wider window.” Remove the repeated explanation about narrow portrait authoring.
Retain accessible names for selection overlays, unlabeled numeric widgets, dialogs,
color inputs and grouped controls even when visual explanatory text is removed.

## Remaining production gates

This pass reduces copy, not feature scope. Complete contextual Help, source-aware
property editing, keyboard/accessibility and real dependency repair still require
production qualification. Revisit strings in actual empty/error/offline/applying
journeys; the current prototype cannot establish those unimplemented states.
