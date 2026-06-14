# Karma

Karma can activate, deactivate, or neutralize a preconfigured Transfer by changing `transfer.quantity`.

Karma does not create Transfer parties, visibility, proposal shape, items, interactions, agreement, or settlement. Those facts are configured as Transfer data first. Karma only changes the Transfer's activation quantity.

## Transfer Quantity Tokens

Transfer quantity is exposed to Karma with two equivalent token forms:

```text
tq4
transfer-quantity-4
```

Both tokens read or write `transfer.quantity` for Transfer `4`.

In a condition, the token is replaced with the current Transfer quantity. If the Transfer does not exist, the value is `0`.

In a consequence, the token identifies which Transfer quantity receives the evaluated condition value.

Examples:

```text
condition: rq7 < 7
operator: =
consequence: tq4
```

If Record `7` is below `7`, Transfer `4` receives quantity `1`.

```text
condition: rq7 - 7
operator: =*
consequence: transfer-quantity-4
```

Transfer `4` receives the numeric difference between Record `7` and `7`.

## Cascading

Karma rules can depend on Transfer quantities:

```text
condition: tq4
operator: =
consequence: rq9
```

When Transfer `4` quantity changes, Karma rules that reference `tq4` or `transfer-quantity-4` in their condition can run. This mirrors the existing `rq{id}` behavior for Record quantity.

This keeps automation bounded and makes Transfer visibility, parties, and settlement inspectable before Karma activates or deactivates it.
