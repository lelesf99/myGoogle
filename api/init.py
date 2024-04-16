from flask import Flask
from models import db

def create_app():
    app = Flask(__name__)
    app.config['SQLALCHEMY_DATABASE_URI'] = 'sqlite:///mygoogle.db'
    app.config['SQLALCHEMY_TRACK_MODIFICATIONS'] = False
    db.init_app(app)
    return app

def init_db():
    app = create_app()
    with app.app_context():
        db.drop_all()  # Be cautious with this in production
        db.create_all()
        print("Database initialized!")

if __name__ == '__main__':
    init_db()
