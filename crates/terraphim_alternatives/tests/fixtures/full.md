# Why isn't everything obvious?

The struggle in a café draft 𝄞 is an eraser holding `code` together.

This whole paragraph might go. It hedges and repeats itself.


```terraphim-alternatives
{
  "version": 1,
  "spans": [
    {
      "id": "s1",
      "kind": "sentence",
      "anchor": {
        "start": 2,
        "end": 31,
        "text": "Why isn't everything obvious?"
      },
      "active": 1,
      "alts": [
        {
          "text": "Shouldn't everything be obvious?",
          "source": "original"
        },
        {
          "text": "Why isn't everything obvious?",
          "source": "human"
        },
        {
          "text": "Everything should be obvious.",
          "source": "ai",
          "model": "terraphim-thesaurus"
        }
      ],
      "ghost": false
    },
    {
      "id": "s2",
      "kind": "word",
      "anchor": {
        "start": 37,
        "end": 45,
        "text": "struggle"
      },
      "active": 3,
      "alts": [
        {
          "text": "tension",
          "source": "original"
        },
        {
          "text": "pressure",
          "source": "human"
        },
        {
          "text": "challenge",
          "source": "human"
        },
        {
          "text": "struggle",
          "source": "ai",
          "model": "llama3"
        },
        {
          "text": "friction",
          "source": "ai",
          "model": "llama3"
        }
      ],
      "ghost": false
    },
    {
      "id": "s3",
      "kind": "word",
      "anchor": {
        "start": 71,
        "end": 77,
        "text": "eraser"
      },
      "active": 1,
      "alts": [
        {
          "text": "paperclip",
          "source": "original"
        },
        {
          "text": "eraser",
          "source": "human"
        },
        {
          "text": "thumbtack",
          "source": "human"
        }
      ],
      "ghost": false
    },
    {
      "id": "s4",
      "kind": "paragraph",
      "anchor": {
        "start": 104,
        "end": 164,
        "text": "This whole paragraph might go. It hedges and repeats itself."
      },
      "active": 0,
      "alts": [
        {
          "text": "This whole paragraph might go. It hedges and repeats itself.",
          "source": "original"
        }
      ],
      "ghost": true
    }
  ],
  "overflow": "Stashed idea.\n\n\u0060\u0060\u0060rust\nfn main() {}\n\u0060\u0060\u0060\nhttps://example.com/a?b=c\n"
}
```
