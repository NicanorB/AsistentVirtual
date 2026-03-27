from __future__ import annotations

from typing import List

from fastapi import FastAPI
from pydantic import BaseModel

EMBEDDING_DIMENSIONS = 1024

app = FastAPI(title="Mock Embedding Service")


class EmbeddingRequest(BaseModel):
    content: str


class EmbeddingResponse(BaseModel):
    index: int
    embedding: List[List[float]]


def build_embedding(
    _text: str, dimensions: int = EMBEDDING_DIMENSIONS
) -> List[List[float]]:
    return [[0.0] * dimensions]


@app.get("/health")
async def health() -> dict[str, str]:
    return {"status": "ok"}


@app.post("/embedding", response_model=List[EmbeddingResponse])
async def embedding(request: EmbeddingRequest) -> List[EmbeddingResponse]:
    return [
        EmbeddingResponse(
            index=0,
            embedding=build_embedding(request.content),
        )
    ]
