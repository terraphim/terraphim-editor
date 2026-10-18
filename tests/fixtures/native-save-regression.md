Every choice is a judgment. A large language model can draft it, but the decision is yours.
A café visit is an honour. Drop this aside.
Pass me a binder clip and a rubber.

I think the plan is perhaps quite good, really. We recieve the report ,and it is basically fine.

```terraphim-alternatives
{
  "version": 1,
  "spans": [
    {
      "id": "s1",
      "kind": "word",
      "anchor": {
        "start": 146,
        "end": 157,
        "text": "binder clip",
        "before": " Drop this aside.\nPass me a ",
        "after": " and a rubber.\n\nI think the plan"
      },
      "active": 1,
      "alts": [
        {
          "text": "paperclip",
          "source": "original"
        },
        {
          "text": "binder clip",
          "source": "human"
        }
      ]
    }
  ],
  "ghosts": [
    {
      "id": "g1",
      "anchor": {
        "start": 119,
        "end": 135,
        "text": "Drop this aside.",
        "before": "\nA café visit is an honour. ",
        "after": "\nPass me a binder clip and a "
      }
    }
  ]
}
```
