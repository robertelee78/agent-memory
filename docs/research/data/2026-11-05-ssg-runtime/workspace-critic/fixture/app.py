"""Synthetic static-analysis fixture; never import, serve, or execute this file."""
import ast
from flask import Flask, request

app = Flask(__name__)


def evaluate_text(text):
    return eval(text)


@app.get("/direct")
def direct():
    return str(evaluate_text(request.args["expression"]))


@app.get("/constant")
def constant():
    return str(eval("1 + 1"))


@app.get("/literal")
def literal():
    return str(ast.literal_eval(request.args["literal"]))


def orphan(text):
    return eval(text)
