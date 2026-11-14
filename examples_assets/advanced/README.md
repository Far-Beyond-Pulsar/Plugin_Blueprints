# Advanced Blueprint examples

Thirteen working Blueprint classes, one trait and one macro library class that show how larger gameplay is
built. Every graph is fully commented: a **yellow box** at the top of each graph explains the idea, and a
coloured **step box** around each group of nodes says what that group does and why. Read the boxes first,
then follow the wires. The basics are in `../basics/`.

These files live in the plugin's read-only folder. To experiment, copy the `.class` folders (and the
`traits/` file) into your project, for example into `src/classes/`, then compile with **F7**.

## Index

| Class | Shows | Look at |
|---|---|---|
| `AdvActorLifecycle.class` | Component-style actor: `begin_play`, `on_tick` with `delta_time`, key input, runtime spawn and despawn, `end_play` cleanup | Sequence node, `Transform` nodes with an unconnected `self`, `entity::spawn` / `entity::despawn` |
| `AdvStateMachine.class` | Finite state machine: one transition event, Switch on Int per state, a "from any state" transition from `Damage` | `change_state` custom event, `Switch on Int`, per-state timers |
| `AdvInteractableDoor.class`, `AdvInteractableLamp.class` | **Trait implementers**: two unrelated classes satisfy one contract (`traits/AdvInteractable.trait.json`) | Class Settings > Traits, `blueprint_metadata.implemented_trait_ids`, methods as custom events |
| `AdvEventDispatchers.class` | Custom events with typed fields: dispatch, handle, **broadcast** to the world, listen to engine events | `Call`/`On`/`Broadcast` nodes, `On GameBegan` |
| `AdvEnemyBase.class`, `AdvEnemyBoss.class` | Inheritance by contract (`parent_class`, identical events) and extension (armor, phases, minions) | Do Once guard, `For Loop`, same event names in both classes |
| `AdvAsyncFlows.class` | Latent flows: `Delay`, `Retriggerable Delay` (debounce), looping timers and `On TimerFired`, `wait::frames` | Delay chain, `timer::set` / `timer::clear` |
| `AdvDataDriven.class` | Data-driven logic: a text table parsed into arrays, array and map nodes, Vec3 values | `Split`, `Array` nodes, `HashMap` nodes, a manual loop index |
| `AdvPlayerStats.class`, `AdvPickup.class`, `AdvHazard.class` | **A small complete game**: coins and medkits score or heal, spikes hurt, win at 50 points, lose at 0 health | Cross-class `Send <Event> to`, the engine `Damage` event, `On BeginOverlap` |
| `AdvMacroLibrary.class` | Reusable macros as a function library: `PrintTagged`, `ClampedAdd`, `Debounce`, `Repeat` (with a body wire) | The macro tabs, macro pins, one exec input with two exec outputs |

## Concepts at a glance

* **Variables** hold per-instance state. They can be `bool`, integers, floats, `String`, `Entity` and object types such as
  `Vec3`. Collections (arrays, maps) are values on wires, built and consumed inside one exec chain.
* **Exec wires** (white) run in order; **data wires** (coloured) carry values. Pure nodes (no exec pins) run where their
  value is used; impure nodes (`Set`, `log`, `array_push`) run on the exec wire and keep their outputs.
* **Events** come in four kinds: lifecycle (`begin_play`, `on_tick`, `on_end_play`), engine events (`On KeyDown`,
  `On BeginOverlap`, `On Damage`, `On TimerFired`, `On GameBegan`), your own custom events, and input actions.
* **Event scope**: `self` hears events raised on this object, `global` hears the whole world, `class` hears every instance of
  the class. The editor picks a sensible default; the node's `scope` property overrides it.
* **Traits** are contracts shared by unrelated classes (see the two interactables). **Macros** are inlined sub-graphs for
  reusable flow shapes. **Custom events** are named calls that can be latent and are visible to other objects.

## Compile order

`AdvPickup` sends `AdvPlayerStats` events, so its compile reads the compiled events of `AdvPlayerStats`. In an editor
session press **Compile All**, or compile `AdvPlayerStats` first. The two interactables need
`traits/AdvInteractable.trait.json` in the project (any folder; the project indexes every `.trait.json`).

## Tips for reading a graph

1. Start at the yellow box, then find the red event nodes: each one is an entry point.
2. Follow the white exec wire from an event to see what runs, in order.
3. Pull data wires backwards from a pin to see where its value comes from.
4. Open the **Variables** panel to see each variable's default value and its description.
5. Change a literal, compile, press Play, and watch the log lines: each example prints what it is doing.
