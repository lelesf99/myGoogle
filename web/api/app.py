import mmap
import os
from flask import Flask, request, jsonify, send_from_directory
from flask_socketio import SocketIO, emit
from models import db, Document
from flask_cors import CORS

app = Flask(__name__)
app.config['SQLALCHEMY_DATABASE_URI'] = 'sqlite:///mygoogle.db'
app.config['SQLALCHEMY_TRACK_MODIFICATIONS'] = False
app.config['UPLOAD_FOLDER'] = 'uploaded_files'
app.config['MAX_CONTENT_LENGTH'] = 8 * 1000 * 1000 * 1000  # 16 Gigabyte limit

db.init_app(app)
CORS(app)
socketio = SocketIO(app, cors_allowed_origins="*")


# In-memory storage for tracking file chunks
file_chunks = {}
assembly_locks = {}

@app.route('/list', methods=['GET'])
def list_files():
    files = Document.query.all()
    return jsonify([file.serialize() for file in files])

@app.route('/search_old', methods=['GET'])
def search_files():
    search_query = request.args.get('query')
    if(search_query == None or search_query == ""):
        return jsonify([])
    files = Document.query.all()
    
    # Open each file and search for the query
    search_results = []
    for file_db in files:
        if(os.path.isfile(file_db.path)):
            with open(os.path.realpath(file_db.path), 'rb', 0) as file_bytes:
                s = mmap.mmap(file_bytes.fileno(), 0, access=mmap.ACCESS_READ)
                # convert search_query to bytes
                search_bytes = search_query.encode('utf-8')
                # search for every occurence of search_query in the file
                index = s.find(search_bytes)
                if(index != -1):
                    result = {"fileName": file_db.name, "file_dbPath": file_db.path, "occurences": []}        
                    while index != -1:
                        # save string near occurence
                        result["occurences"].append({
                            "start": index,
                            "end": index + len(search_bytes),
                            "context": s[index - 20:index + 20].decode('utf-8', errors='ignore')
                        })
                        index = s.find(search_bytes, index + 1)
                    search_results.append(result)
        else:
            print(f"File {file_db.name} not found at {file_db.path}")
            db.session.delete(file_db)
            db.session.commit()
                
    return jsonify(search_results)




# download file
@app.route('/uploaded_files/<path:file_name>', methods=['GET'])
def download_file(file_name):
    file = Document.query.filter_by(name=file_name).first()
    if file:
        uploads = os.path.join(app.root_path, app.config['UPLOAD_FOLDER'])
        return send_from_directory(directory=uploads, path=file_name)
    return jsonify({'message': 'File not found'})

# delete
@app.route('/delete', methods=['DELETE'])
def delete_file():
    file_name = request.args.get('fileName')
    file = Document.query.filter_by(name=file_name).first()
    if file:
        db.session.delete(file)
        db.session.commit()
        os.remove(file.path)
        return jsonify({'message': 'File deleted'})
    return jsonify({'message': 'File not found'})

@app.route('/upload', methods=['POST'])
def upload_file():
    file = request.files['file']
    file_name = file.filename
    file_path = os.path.join(app.config['UPLOAD_FOLDER'], file_name)
    file.save(file_path)
    # add or update file to database
    new_file = Document.query.filter_by(name=file_name).first()
    if new_file:
        new_file.path = file_path
    else:
        new_file = Document(name=file_name, path=file_path)
        db.session.add(new_file)
    db.session.commit()

    return jsonify(new_file.serialize())

@socketio.on('connect')
def handle_connect():
    print('Client connected')

@socketio.on('disconnect')
def handle_disconnect():
    print('Client disconnected')

@socketio.on('search')
def handle_search(query):
    files = Document.query.all()
    total_bytes = 0
    byte_counter = 0
    for file in files:
        total_bytes += os.path.getsize(file.path)
    emit('search_progress', {'searched': 0,'totalBytes': total_bytes})
    for file in files:
        if(os.path.isfile(file.path)):
            with open(os.path.realpath(file.path), 'rb', 0) as file_bytes:
                s = mmap.mmap(file_bytes.fileno(), 0, access=mmap.ACCESS_READ)
                # convert search_query to bytes
                search_bytes = query.encode('utf-8')
                # search for every occurence of search_query in the file
                index = s.find(search_bytes)
                if(index != -1):
                    emit('result', {"fileName": file.name, "file_dbPath": file.path, "occurences": []})    
                    while index != -1:
                        # save string near occurence
                        emit('occurence', {"fileName": file.name, "occurence": {
                                "start": index,
                                "end": index + len(search_bytes),
                                "context": s[index - 20:index + 20].decode('utf-8', errors='ignore')
                            }
                        })
                        emit('search_progress', {'searched': byte_counter + index,'totalBytes': total_bytes})
                        index = s.find(search_bytes, index + 1)
                byte_counter += os.path.getsize(file.path)
                emit('search_progress', {'searched': byte_counter + index,'totalBytes': total_bytes})
    emit('search_progress', {'searched': total_bytes,'totalBytes': total_bytes})
    emit('close_connection', 'done')

# debug mode
if __name__ == '__main__':
    socketio.run(app, debug=True)

# if __name__ == '__main__':
#     app.run(host='0.0.0.0', port=5000)
