"""Synthetic parser input only; no test runner or application server is invoked."""
from flask import Flask, request

app = Flask(__name__)


@app.get("/test-only")
def test_only():
    return eval(request.args["expression"])
