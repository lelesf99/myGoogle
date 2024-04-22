from flask_sqlalchemy import SQLAlchemy

db = SQLAlchemy()

class Document(db.Model):
    id = db.Column(db.Integer, primary_key=True)
    name = db.Column(db.String(255), nullable=False)
    path = db.Column(db.String(255), nullable=False)
    
    def serialize(self):
        return {"id": self.id,
                "fileName": self.name,
                "filePath": self.path}

    def __repr__(self):
        return f'<Document {self.name}>'
