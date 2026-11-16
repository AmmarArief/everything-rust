"""Word utilities, written the way Python code often is."""
import re
import unicodedata
from typing import Optional

_SEPARATORS = re.compile(r"[^\w]+")


def slugify(text: str, sep: str = "-", *, max_length: Optional[int] = None) -> str:
    """Lower-cased, accents stripped, words joined by `sep`."""
    normalized = unicodedata.normalize("NFKD", text)
    ascii_only = "".join(c for c in normalized if not unicodedata.combining(c))
    words = [w for w in _SEPARATORS.split(ascii_only.lower()) if w]
    slug = sep.join(words)
    if max_length is not None:
        slug = slug[:max_length].rstrip(sep)
    return slug


def word_counts(text: str, min_length: int = 1) -> dict[str, int]:
    """Occurrences of each word, in first-seen order."""
    counts: dict[str, int] = {}
    for word in text.split():
        word = word.strip(".,;:!?\"'()").lower()
        if len(word) >= min_length:
            counts[word] = counts.get(word, 0) + 1
    return counts


def chunk(items: list[int], size: int) -> list[list[int]]:
    """Consecutive slices of `size` items; the last may be shorter."""
    if size <= 0:
        raise ValueError("size must be positive")
    return [items[i:i + size] for i in range(0, len(items), size)]


def page_count(total: int, per_page: int) -> int:
    """Pages needed for `total` items: floor division the Python way."""
    return -(-total // per_page)


def mean(values: list[float]) -> float:
    return sum(values) / len(values)


def _private_helper(x):
    return x


def walk(items):
    for item in items:
        yield item


def apply(fn, value):
    return fn(value)


def configure(**options):
    return options
