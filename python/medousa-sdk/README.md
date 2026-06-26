# medousa-sdk (Python)

Python client for **medousa_daemon**. Full documentation: [docs/sdk/python.md](../../docs/sdk/python.md).

```bash
pip install -e "python/medousa-sdk[dev]"
```

```python
import asyncio
from medousa import MedousaClient

async def main():
    async with MedousaClient("http://127.0.0.1:7419") as client:
        health = await client.health().get()
        print(health.status)

asyncio.run(main())
```
